use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::check_cancelled;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) struct Identity {
    device: libc::dev_t,
    inode: libc::ino_t,
}

impl From<&libc::stat> for Identity {
    fn from(metadata: &libc::stat) -> Self {
        Self {
            device: metadata.st_dev,
            inode: metadata.st_ino,
        }
    }
}

pub(super) struct Directory {
    pub fd: Arc<OwnedFd>,
    pub names: Vec<OsString>,
    pub error: Option<io::Error>,
}

pub(super) enum Operation {
    Stat,
    List,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stat => "Reading filesystem metadata",
            Self::List => "Listing directory",
        })
    }
}

pub(super) struct MetadataAccess<'a> {
    cancelled: &'a AtomicBool,
    #[cfg(target_os = "macos")]
    provider: super::provider::Access<'a>,
}

impl<'a> MetadataAccess<'a> {
    pub(super) fn new(cancelled: &'a AtomicBool) -> Self {
        Self {
            cancelled,
            #[cfg(target_os = "macos")]
            provider: super::provider::Access::new(cancelled),
        }
    }

    pub(super) fn stat(
        &mut self,
        _path: &Path,
        parent: Option<&Arc<OwnedFd>>,
        name: &OsStr,
    ) -> io::Result<libc::stat> {
        #[cfg(target_os = "macos")]
        if super::provider::is_provider_path(_path) {
            let parent = parent.cloned();
            let name = name.to_owned();
            return self
                .provider
                .run(_path, move |_| stat_at(parent.as_deref(), &name));
        }
        check_cancelled(self.cancelled)?;
        stat_at(parent.map(Arc::as_ref), name)
    }

    pub(super) fn list(
        &mut self,
        _path: &Path,
        parent: Option<&Arc<OwnedFd>>,
        name: &OsStr,
        expected: Identity,
    ) -> io::Result<Directory> {
        #[cfg(target_os = "macos")]
        if super::provider::is_provider_path(_path) {
            let parent = parent.cloned();
            let name = name.to_owned();
            return self.provider.run(_path, move |abandoned| {
                list_at(parent.as_deref(), &name, expected, abandoned)
            });
        }
        list_at(parent.map(Arc::as_ref), name, expected, self.cancelled)
    }
}

pub(super) fn with_local_metadata<T>(operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    #[cfg(target_os = "macos")]
    let _materialization = super::provider::LocalMetadataOnly::enter()?;
    operation()
}

/// The traversal keeps one directory descriptor open per level. Processes started by
/// launchd on macOS get a soft limit of 256, so deep trees would otherwise fail with
/// EMFILE. Raise the soft limit without exceeding the hard limit. This never lowers it.
pub(super) fn raise_open_file_limit() {
    // OPEN_MAX from the macOS SDK's <sys/syslimits.h>. It is also ample on Linux.
    const TARGET: libc::rlim_t = 10_240;
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit writes a complete rlimit struct into valid, writable memory.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
        return;
    }
    let target = TARGET.min(limit.rlim_max);
    if limit.rlim_cur < target {
        limit.rlim_cur = target;
        // SAFETY: setrlimit reads a valid rlimit struct and changes only this process's limit.
        let _ = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) };
    }
}

pub(super) fn is_dataless(_metadata: &libc::stat) -> bool {
    #[cfg(target_os = "macos")]
    {
        // SF_DATALESS from the macOS SDK's <sys/stat.h>.
        _metadata.st_flags & 0x4000_0000 != 0
    }
    #[cfg(not(target_os = "macos"))]
    false
}

pub(super) fn is_cloud_error(_error: &io::Error) -> bool {
    #[cfg(target_os = "macos")]
    {
        _error.raw_os_error() == Some(libc::EDEADLK)
    }
    #[cfg(not(target_os = "macos"))]
    false
}

fn stat_at(parent: Option<&OwnedFd>, name: &OsStr) -> io::Result<libc::stat> {
    let name = posix_path(name)?;
    let mut result = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: name is NUL-terminated, result is writable, and any relative
    // descriptor is owned by the caller and stays open throughout the operation.
    let status = unsafe {
        libc::fstatat(
            directory_fd(parent),
            name.as_ptr(),
            result.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if status < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatat succeeded and initialized the struct.
    Ok(unsafe { result.assume_init() })
}

fn list_at(
    parent: Option<&OwnedFd>,
    name: &OsStr,
    expected: Identity,
    cancelled: &AtomicBool,
) -> io::Result<Directory> {
    check_cancelled(cancelled)?;
    let name = posix_path(name)?;
    // SAFETY: name is a C string and the borrowed directory outlives openat.
    // O_NOFOLLOW guards against a directory being replaced by a symlink mid-scan.
    let fd = unsafe {
        libc::openat(
            directory_fd(parent),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned an owned descriptor.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut actual = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: the descriptor is live and actual is writable.
    if unsafe { libc::fstat(fd.as_raw_fd(), actual.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstat succeeded above.
    if Identity::from(&unsafe { actual.assume_init() }) != expected {
        return Err(io::Error::other("Directory changed during the scan."));
    }

    // Give the directory stream its own descriptor. The scan keeps the original
    // for relative metadata calls after enumeration, including provider workers.
    // SAFETY: fcntl borrows a live file descriptor.
    let stream_fd = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if stream_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl returned an owned descriptor.
    let stream_fd = unsafe { OwnedFd::from_raw_fd(stream_fd) };
    let stream = DirectoryStream::open(stream_fd)?;
    let (names, error) = stream.names(cancelled)?;
    Ok(Directory {
        fd: Arc::new(fd),
        names,
        error,
    })
}

fn directory_fd(parent: Option<&OwnedFd>) -> libc::c_int {
    parent.map_or(libc::AT_FDCWD, AsRawFd::as_raw_fd)
}

fn posix_path(path: &OsStr) -> io::Result<CString> {
    CString::new(path.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "A path cannot contain a NUL byte.",
        )
    })
}

struct DirectoryStream(NonNull<libc::DIR>);

impl DirectoryStream {
    fn open(fd: OwnedFd) -> io::Result<Self> {
        // SAFETY: fdopendir receives an open directory descriptor. On success,
        // fdopendir takes ownership, so closedir will close the descriptor.
        let stream = unsafe { libc::fdopendir(fd.as_raw_fd()) };
        let stream = NonNull::new(stream).ok_or_else(io::Error::last_os_error)?;
        let _ = fd.into_raw_fd();
        Ok(Self(stream))
    }

    fn names(&self, cancelled: &AtomicBool) -> io::Result<(Vec<OsString>, Option<io::Error>)> {
        let mut names = Vec::new();
        let error = loop {
            check_cancelled(cancelled)?;
            // SAFETY: this thread owns the stream. POSIX dirent names are NUL-terminated
            // and valid until the next readdir call. Copy each name before advancing it.
            let name = unsafe {
                #[cfg(target_os = "macos")]
                let errno = libc::__error();
                #[cfg(target_os = "linux")]
                let errno = libc::__errno_location();
                *errno = 0;
                let entry = libc::readdir(self.0.as_ptr());
                if entry.is_null() {
                    let code = *errno;
                    break (code != 0).then(|| io::Error::from_raw_os_error(code));
                }
                CStr::from_ptr(std::ptr::addr_of!((*entry).d_name).cast()).to_bytes()
            };
            if name != b"." && name != b".." {
                names.push(OsString::from_vec(name.to_vec()));
            }
        };
        names.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        Ok((names, error))
    }
}

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        // SAFETY: the wrapper owns a unique, open directory stream.
        unsafe { libc::closedir(self.0.as_ptr()) };
    }
}
