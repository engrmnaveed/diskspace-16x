//! A few Objective-C runtime calls, which replace a binding crate for the two Foundation
//! methods DiskUse16x uses: the Trash and, in Pro, a volume's purgeable space.

use std::ffi::{CStr, c_char, c_void};

pub type Object = *mut c_void;
pub type Selector = *const c_void;

/// `BOOL` is a C `bool` on Apple silicon and a `signed char` on Intel.
#[cfg(target_arch = "aarch64")]
pub type ObjcBool = bool;
#[cfg(not(target_arch = "aarch64"))]
pub type ObjcBool = i8;

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

#[link(name = "objc")]
unsafe extern "C" {
    pub fn objc_getClass(name: *const c_char) -> Object;
    pub fn sel_registerName(name: *const c_char) -> Selector;
    pub fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

/// Sends a message through `objc_msgSend` cast to the method's exact signature, which
/// the Apple silicon calling convention requires.
macro_rules! send {
    ($receiver:expr, $selector:expr $(, $argument:expr => $kind:ty)* ; $result:ty) => {{
        use $crate::objc::{Object, Selector, objc_msgSend, sel_registerName};
        let call: unsafe extern "C" fn(Object, Selector $(, $kind)*) -> $result =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        call($receiver, sel_registerName(std::ffi::CStr::as_ptr($selector)) $(, $argument)*)
    }};
}
pub(crate) use send;

/// Runs `body` in an autorelease pool, which releases the autoreleased objects Foundation
/// returns before this returns.
pub fn with_pool<T>(body: impl FnOnce() -> T) -> T {
    // SAFETY: pushes and pops are balanced on this thread.
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let result = body();
        objc_autoreleasePoolPop(pool);
        result
    }
}

/// `+[NSURL fileURLWithFileSystemRepresentation:isDirectory:relativeToURL:]`, which keeps
/// paths that aren't valid UTF-8 intact. Null when the path can't be a file URL.
///
/// # Safety
/// Call inside [`with_pool`].
pub unsafe fn file_url(representation: &CStr) -> Object {
    unsafe {
        send!(
            objc_getClass(c"NSURL".as_ptr()),
            c"fileURLWithFileSystemRepresentation:isDirectory:relativeToURL:",
            representation.as_ptr() => *const c_char,
            ObjcBool::default() => ObjcBool,
            std::ptr::null_mut() => Object;
            Object
        )
    }
}

/// Copies the C string a method such as `UTF8String` returns.
///
/// # Safety
/// `receiver` is null or an object that responds to `selector` with a C string.
pub unsafe fn utf8_bytes(receiver: Object, selector: &CStr) -> Option<Vec<u8>> {
    if receiver.is_null() {
        return None;
    }
    unsafe {
        let text = send!(receiver, selector; *const c_char);
        (!text.is_null()).then(|| CStr::from_ptr(text).to_bytes().to_vec())
    }
}
