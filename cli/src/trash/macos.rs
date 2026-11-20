//! macOS Trash through `-[NSFileManager trashItemAtURL:resultingItemURL:error:]`, the call
//! behind Finder's Move to Trash. A few Objective-C runtime calls replace a binding crate.
//! macOS picks the Trash on the item's volume and renames the item into it.

use std::ffi::{CStr, CString, OsString, c_char, c_void};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;

type Object = *mut c_void;
type Selector = *const c_void;

/// `BOOL` is a C `bool` on Apple silicon and a `signed char` on Intel.
#[cfg(target_arch = "aarch64")]
type ObjcBool = bool;
#[cfg(not(target_arch = "aarch64"))]
type ObjcBool = i8;

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Object;
    fn sel_registerName(name: *const c_char) -> Selector;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

/// Sends a message through `objc_msgSend` cast to the method's exact signature, which
/// the Apple silicon calling convention requires.
macro_rules! send {
    ($receiver:expr, $selector:expr $(, $argument:expr => $kind:ty)* ; $result:ty) => {{
        let call: unsafe extern "C" fn(Object, Selector $(, $kind)*) -> $result =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        call($receiver, sel_registerName(CStr::as_ptr($selector)) $(, $argument)*)
    }};
}

/// `NSCocoaErrorDomain` codes from `FoundationErrors.h`.
const NO_SUCH_FILE: isize = 4;
const READ_NO_PERMISSION: isize = 257;
const WRITE_NO_PERMISSION: isize = 513;

pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    let representation = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Paths can't contain NUL."))?;
    // Foundation returns autoreleased objects. The pool releases them before returning.
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let result = trash(&representation);
        objc_autoreleasePoolPop(pool);
        result
    }
}

unsafe fn trash(representation: &CStr) -> io::Result<PathBuf> {
    unsafe {
        // A file system representation keeps paths that aren't valid UTF-8 intact.
        let url = send!(
            objc_getClass(c"NSURL".as_ptr()),
            c"fileURLWithFileSystemRepresentation:isDirectory:relativeToURL:",
            representation.as_ptr() => *const c_char,
            ObjcBool::default() => ObjcBool,
            ptr::null_mut() => Object;
            Object
        );
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

/// Copies the C string a method such as `UTF8String` returns.
unsafe fn utf8_bytes(receiver: Object, selector: &CStr) -> Option<Vec<u8>> {
    if receiver.is_null() {
        return None;
    }
    unsafe {
        let text = send!(receiver, selector; *const c_char);
        (!text.is_null()).then(|| CStr::from_ptr(text).to_bytes().to_vec())
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
