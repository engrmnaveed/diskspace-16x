use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::{Details, check_cancelled};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Identity {
    device: libc::dev_t,
    inode: libc::ino_t,
}

impl Identity {
    /// An identity read from a saved scan. Values outside the platform's range wrap.
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn new(device: u64, inode: u64) -> Self {
        Self {
            device: device as libc::dev_t,
            inode: inode as libc::ino_t,
        }
    }
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
    /// Entries in bytewise name order.
    pub entries: Vec<DirectoryEntry>,
    pub error: Option<io::Error>,
}

pub(super) struct DirectoryEntry {
    pub name: OsString,
    pub listed: Listed,
}

/// What a directory listing already tells about an entry. Only macOS lists metadata.
#[derive(Clone, Copy)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) enum Listed {
    /// Nothing yet: read the entry's metadata on its own.
    Unknown,
    /// A regular file's metadata, read in bulk with the listing.
    File(FileMetadata),
    /// A symbolic link or special file, which contributes no bytes.
    Special,
}

/// The metadata the scan counts for a regular file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FileMetadata {
    pub device: libc::dev_t,
    pub identity: Identity,
    /// The file has more than one hard link.
    pub linked: bool,
    /// The file length (`st_size`).
    pub size: u64,
    /// Allocated bytes (`st_blocks` × 512), or None beyond the byte range.
    pub allocated: Option<u64>,
    pub dataless: bool,
    /// Pro's modification time and owner.
    pub details: Details,
}

impl From<&libc::stat> for FileMetadata {
    fn from(metadata: &libc::stat) -> Self {
        Self {
            device: metadata.st_dev,
            identity: Identity::from(metadata),
            linked: metadata.st_nlink > 1,
            size: metadata.st_size.max(0) as u64,
            // POSIX st_blocks describes 512-byte units, regardless of the filesystem block size.
            allocated: (metadata.st_blocks.max(0) as u64).checked_mul(512),
            dataless: is_dataless(metadata),
            details: Details::from_stat(metadata),
        }
    }
}

pub(super) enum Operation {
    Stat,
    List,
    #[cfg(target_os = "macos")]
    CanonicalPath,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stat => "Reading filesystem metadata",
            Self::List => "Listing directory",
            #[cfg(target_os = "macos")]
            Self::CanonicalPath => "Checking for an APFS firmlink",
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

    #[cfg(target_os = "macos")]
    pub(super) fn canonical_directory_path(
        &mut self,
        path: &Path,
        parent: &Arc<OwnedFd>,
        name: &OsStr,
    ) -> io::Result<std::path::PathBuf> {
        use super::startup_volume::canonical_path_at;

        if super::provider::is_provider_path(path) {
            let parent = Arc::clone(parent);
            let name = name.to_owned();
            return self
                .provider
                .run(path, move |_| canonical_path_at(&parent, &name));
        }
        canonical_path_at(parent, name)
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

    #[cfg(target_os = "macos")]
    if let Some((entries, error)) = bulk::read(&fd, cancelled)? {
        return Ok(Directory {
            fd: Arc::new(fd),
            entries,
            error,
        });
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
    let entries = names
        .into_iter()
        .map(|name| DirectoryEntry {
            name,
            listed: Listed::Unknown,
        })
        .collect();
    Ok(Directory {
        fd: Arc::new(fd),
        entries,
        error,
    })
}

fn directory_fd(parent: Option<&OwnedFd>) -> libc::c_int {
    parent.map_or(libc::AT_FDCWD, AsRawFd::as_raw_fd)
}

pub(super) fn posix_path(path: &OsStr) -> io::Result<CString> {
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

/// Reads a directory's names, and regular files' metadata, with `getattrlistbulk`.
/// One call returns many entries without looking each one up, which keeps large scans
/// from thrashing the kernel's vnode cache. Folders still get their own `fstatat`, so
/// firmlinks and mount points resolve as they always have.
#[cfg(target_os = "macos")]
mod bulk {
    use std::cell::RefCell;
    use std::ffi::OsStr;
    use std::io;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::AtomicBool;

    use super::{Details, DirectoryEntry, FileMetadata, Identity, Listed, check_cancelled};

    // From the macOS SDK's <sys/attr.h> and <sys/vnode.h>.
    const ATTR_CMN_ERROR: u32 = 0x2000_0000;
    /// Pro also reads each file's modification time and owner.
    const DETAILS: u32 = if cfg!(feature = "pro") {
        libc::ATTR_CMN_MODTIME | libc::ATTR_CMN_OWNERID
    } else {
        0
    };
    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const SF_DATALESS: u32 = 0x4000_0000;
    const BUFFER_BYTES: usize = 64 * 1024;

    thread_local! {
        static BUFFER: RefCell<Vec<u8>> = RefCell::new(vec![0; BUFFER_BYTES]);
    }

    /// The entries in name order and any error that cut the listing short, or None
    /// when the filesystem can't list this directory in bulk.
    pub(super) fn read(
        fd: &OwnedFd,
        cancelled: &AtomicBool,
    ) -> io::Result<Option<(Vec<DirectoryEntry>, Option<io::Error>)>> {
        let mut request = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: libc::ATTR_CMN_RETURNED_ATTRS
                | libc::ATTR_CMN_NAME
                | libc::ATTR_CMN_DEVID
                | libc::ATTR_CMN_OBJTYPE
                | libc::ATTR_CMN_FLAGS
                | libc::ATTR_CMN_FILEID
                | ATTR_CMN_ERROR
                | DETAILS,
            volattr: 0,
            dirattr: 0,
            fileattr: libc::ATTR_FILE_LINKCOUNT
                | libc::ATTR_FILE_ALLOCSIZE
                | libc::ATTR_FILE_DATALENGTH,
            forkattr: 0,
        };
        BUFFER.with_borrow_mut(|buffer| {
            let mut entries = Vec::new();
            let mut started = false;
            let error = loop {
                check_cancelled(cancelled)?;
                // SAFETY: the descriptor is open, the request is a valid attrlist, and
                // the buffer is writable for its full length.
                let count = unsafe {
                    libc::getattrlistbulk(
                        fd.as_raw_fd(),
                        std::ptr::from_mut(&mut request).cast(),
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                        0,
                    )
                };
                if count < 0 {
                    if !started {
                        return Ok(None);
                    }
                    break Some(io::Error::last_os_error());
                }
                if count == 0 {
                    break None;
                }
                started = true;
                if let Err(error) = parse(&buffer[..], count as usize, &mut entries) {
                    break Some(error);
                }
            };
            entries.sort_unstable_by(|left: &DirectoryEntry, right| {
                left.name.as_bytes().cmp(right.name.as_bytes())
            });
            Ok(Some((entries, error)))
        })
    }

    fn parse(buffer: &[u8], count: usize, entries: &mut Vec<DirectoryEntry>) -> io::Result<()> {
        let mut offset = 0;
        for _ in 0..count {
            let length = Reader::new(buffer, offset).u32()? as usize;
            let record = buffer
                .get(offset..offset + length)
                .filter(|_| length >= 4)
                .ok_or_else(invalid)?;
            if let Some(entry) = entry(record)? {
                entries.push(entry);
            }
            offset += length;
        }
        Ok(())
    }

    /// One packed entry: its length, the attributes returned, then each returned
    /// attribute in a fixed order.
    fn entry(record: &[u8]) -> io::Result<Option<DirectoryEntry>> {
        let mut reader = Reader::new(record, 4);
        let common = reader.u32()?;
        let _volume = reader.u32()?;
        let _directory = reader.u32()?;
        let file = reader.u32()?;
        let _fork = reader.u32()?;
        let error = if common & ATTR_CMN_ERROR != 0 {
            reader.u32()?
        } else {
            0
        };
        if common & libc::ATTR_CMN_NAME == 0 {
            return Ok(None);
        }
        let name = reader.name()?;
        if name == b"." || name == b".." || name.is_empty() {
            return Ok(None);
        }
        let mut take = |bit: u32, set: u32, wide: bool| -> io::Result<Option<u64>> {
            if set & bit == 0 {
                return Ok(None);
            }
            if wide {
                reader.u64().map(Some)
            } else {
                reader.u32().map(|value| Some(u64::from(value)))
            }
        };
        let device = take(libc::ATTR_CMN_DEVID, common, false)?;
        let kind = take(libc::ATTR_CMN_OBJTYPE, common, false)?;
        // A timespec: seconds, then nanoseconds.
        let modified = take(libc::ATTR_CMN_MODTIME, common, true)?;
        let _nanoseconds = take(libc::ATTR_CMN_MODTIME, common, true)?;
        let owner = take(libc::ATTR_CMN_OWNERID, common, false)?;
        let flags = take(libc::ATTR_CMN_FLAGS, common, false)?;
        let inode = take(libc::ATTR_CMN_FILEID, common, true)?;
        let links = take(libc::ATTR_FILE_LINKCOUNT, file, false)?;
        let allocated = take(libc::ATTR_FILE_ALLOCSIZE, file, true)?;
        let size = take(libc::ATTR_FILE_DATALENGTH, file, true)?;
        let listed = match (error, kind) {
            (0, Some(kind)) if kind == u64::from(VREG) => {
                match (device, flags, inode, links, allocated, size) {
                    (
                        Some(device),
                        Some(flags),
                        Some(inode),
                        Some(links),
                        Some(allocated),
                        Some(size),
                    ) => {
                        let device = device as u32 as libc::dev_t;
                        Listed::File(FileMetadata {
                            device,
                            identity: Identity {
                                device,
                                inode: inode as libc::ino_t,
                            },
                            linked: links > 1,
                            // off_t values; a negative one would be invalid.
                            size: (size as i64).max(0) as u64,
                            allocated: Some((allocated as i64).max(0) as u64),
                            dataless: flags as u32 & SF_DATALESS != 0,
                            details: details(modified, owner),
                        })
                    }
                    _ => Listed::Unknown,
                }
            }
            (0, Some(kind)) if kind != u64::from(VDIR) => Listed::Special,
            _ => Listed::Unknown,
        };
        Ok(Some(DirectoryEntry {
            name: OsStr::from_bytes(name).to_owned(),
            listed,
        }))
    }

    #[cfg(feature = "pro")]
    fn details(modified: Option<u64>, owner: Option<u64>) -> Details {
        match modified {
            // time_t seconds, which are signed.
            Some(seconds) => Details::new(seconds as i64, owner.map_or(0, |owner| owner as u32)),
            None => Details::default(),
        }
    }

    #[cfg(not(feature = "pro"))]
    fn details(_modified: Option<u64>, _owner: Option<u64>) -> Details {
        Details::default()
    }

    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl<'a> Reader<'a> {
        fn new(bytes: &'a [u8], at: usize) -> Self {
            Self { bytes, at }
        }

        fn take<const N: usize>(&mut self) -> io::Result<[u8; N]> {
            let bytes = self.bytes.get(self.at..self.at + N).ok_or_else(invalid)?;
            self.at += N;
            Ok(bytes.try_into().expect("the slice has N bytes"))
        }

        fn u32(&mut self) -> io::Result<u32> {
            self.take().map(u32::from_ne_bytes)
        }

        fn u64(&mut self) -> io::Result<u64> {
            self.take().map(u64::from_ne_bytes)
        }

        /// An attrreference: an offset from its own position and a length that
        /// includes the terminating NUL.
        fn name(&mut self) -> io::Result<&'a [u8]> {
            let position = self.at;
            let offset = self.u32()? as i32;
            let length = self.u32()? as usize;
            let start = position
                .checked_add_signed(offset as isize)
                .ok_or_else(invalid)?;
            let bytes = self.bytes.get(start..start + length).ok_or_else(invalid)?;
            match bytes.split_last() {
                Some((0, name)) => Ok(name),
                _ => Err(invalid()),
            }
        }
    }

    fn invalid() -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "macOS returned an invalid directory listing.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn lists_names_in_order_with_the_same_file_metadata_as_fstatat() -> io::Result<()> {
        let root = std::env::temp_dir().join(format!("diskuse16x-listing-{}", std::process::id()));
        fs::create_dir(&root)?;
        let result = (|| {
            fs::create_dir(root.join("folder"))?;
            fs::write(root.join("b.bin"), vec![1; 70_000])?;
            fs::write(root.join("a.txt"), "small")?;
            fs::write(root.join("empty"), "")?;
            fs::File::create(root.join("sparse.img"))?.set_len(16 * 1024 * 1024)?;
            fs::hard_link(root.join("b.bin"), root.join("link.bin"))?;
            symlink(root.join("a.txt"), root.join("pointer"))?;
            let cancelled = AtomicBool::new(false);
            let metadata = stat_at(None, root.as_os_str())?;
            let directory = list_at(
                None,
                root.as_os_str(),
                Identity::from(&metadata),
                &cancelled,
            )?;
            assert!(directory.error.is_none());
            let names: Vec<_> = directory
                .entries
                .iter()
                .map(|entry| entry.name.to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                names,
                [
                    "a.txt",
                    "b.bin",
                    "empty",
                    "folder",
                    "link.bin",
                    "pointer",
                    "sparse.img"
                ]
            );
            for entry in &directory.entries {
                let stat = stat_at(Some(&directory.fd), &entry.name)?;
                let regular = stat.st_mode & libc::S_IFMT == libc::S_IFREG;
                match entry.listed {
                    Listed::File(file) => assert_eq!(file, FileMetadata::from(&stat)),
                    Listed::Special => assert_eq!(stat.st_mode & libc::S_IFMT, libc::S_IFLNK),
                    // Folders always get their own fstatat. Linux reads everything that way.
                    Listed::Unknown => assert!(!regular || cfg!(not(target_os = "macos"))),
                }
                if cfg!(target_os = "macos") {
                    assert_eq!(
                        regular,
                        matches!(entry.listed, Listed::File(_)),
                        "{names:?}"
                    );
                }
            }
            Ok(())
        })();
        fs::remove_dir_all(&root)?;
        result
    }
}
