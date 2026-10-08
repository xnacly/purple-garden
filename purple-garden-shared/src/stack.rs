//! Bounds of the calling thread's stack, so native code can trap before it
//! runs off the end.

use std::ffi::c_void;

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn pthread_self() -> usize;
    fn pthread_getattr_np(thread: usize, attr: *mut PthreadAttr) -> i32;
    fn pthread_attr_getstack(
        attr: *const PthreadAttr,
        addr: *mut *mut c_void,
        size: *mut usize,
    ) -> i32;
    fn pthread_attr_destroy(attr: *mut PthreadAttr) -> i32;
}

/// Room for glibc's `pthread_attr_t` on every supported target.
#[cfg(target_os = "linux")]
#[repr(C, align(8))]
struct PthreadAttr([u8; 64]);

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_self() -> usize;
    fn pthread_get_stackaddr_np(thread: usize) -> *mut c_void;
    fn pthread_get_stacksize_np(thread: usize) -> usize;
}

/// Lowest address of the calling thread's stack.
#[cfg(target_os = "linux")]
#[must_use]
pub fn thread_stack_low() -> Option<usize> {
    let mut attr = PthreadAttr([0; 64]);
    unsafe {
        if pthread_getattr_np(pthread_self(), &mut attr) != 0 {
            return None;
        }
        let (mut addr, mut size) = (std::ptr::null_mut(), 0);
        let found = pthread_attr_getstack(&attr, &mut addr, &mut size) == 0;
        pthread_attr_destroy(&mut attr);
        found.then_some(addr as usize)
    }
}

/// Lowest address of the calling thread's stack. macOS reports the top.
#[cfg(target_os = "macos")]
#[must_use]
pub fn thread_stack_low() -> Option<usize> {
    unsafe {
        let thread = pthread_self();
        Some(pthread_get_stackaddr_np(thread) as usize - pthread_get_stacksize_np(thread))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stack_low_is_below_a_local() {
        let local = 0u8;
        let low = super::thread_stack_low().expect("stack bounds");
        assert!(low < &raw const local as usize);
    }
}
