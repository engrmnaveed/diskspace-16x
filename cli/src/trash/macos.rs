//! macOS Trash through `-[NSFileManager trashItemAtURL:resultingItemURL:error:]`, the call
//! behind Finder's Move to Trash, sent through [`crate::objc`]. macOS picks the Trash on
//! the item's volume and renames the item into it.

use std::ffi::{CStr, CString, OsString};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;

use crate::objc::{ObjcBool, Object, file_url, objc_getClass, send, utf8_bytes, with_pool};

/// `NSCocoaErrorDomain` codes from `FoundationErrors.h`.
const NO_SUCH_FILE: isize = 4;
const READ_NO_PERMISSION: isize = 257;
const WRITE_NO_PERMISSION: isize = 513;

pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    let representation = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Paths can't contain NUL."))?;
    // SAFETY: trash runs inside the pool that releases Foundation's objects.
    with_pool(|| unsafe { trash(&representation) })
}

unsafe fn trash(representation: &CStr) -> io::Result<PathBuf> {
    unsafe {
        let url = file_url(representation);
        if url.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "This path can't be expressed as a file URL.",
            ));
        }
        let manager = send!(objc_getClass(c"NSFileManager".as_ptr()), c"defaultManager"; Object);
        let mut resulting: Object = ptr::null_mut();
        let mut error: Object = ptr::null_mut();
        let moved = send!(
            manager,
            c"trashItemAtURL:resultingItemURL:error:",
            url => Object,
            &mut resulting => *mut Object,
            &mut error => *mut Object;
            ObjcBool
        );
        if moved == ObjcBool::default() {
            return Err(if error.is_null() {
                io::Error::other("macOS didn't move the item to the Trash.")
            } else {
                ns_error(error)
            });
        }
        let location = utf8_bytes(resulting, c"fileSystemRepresentation").unwrap_or_default();
        Ok(OsString::from_vec(location).into())
    }
}

/// Converts an `NSError` to an `io::Error` that carries its localized description.
unsafe fn ns_error(error: Object) -> io::Error {
    unsafe {
        let code = send!(error, c"code"; isize);
        let domain = utf8_bytes(send!(error, c"domain"; Object), c"UTF8String");
        let description = utf8_bytes(send!(error, c"localizedDescription"; Object), c"UTF8String")
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|| "macOS didn't move the item to the Trash.".into());
        let kind = match (domain.as_deref(), code) {
            (Some(b"NSCocoaErrorDomain"), NO_SUCH_FILE) => io::ErrorKind::NotFound,
            (Some(b"NSCocoaErrorDomain"), READ_NO_PERMISSION | WRITE_NO_PERMISSION) => {
                io::ErrorKind::PermissionDenied
            }
            (Some(b"NSPOSIXErrorDomain"), _) => io::Error::from_raw_os_error(code as i32).kind(),
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, description)
    }
}
