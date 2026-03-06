use std::sync::atomic::AtomicU32;

pub fn futex_wait(atomic: &AtomicU32, expected: u32) {
    #[cfg(target_os = "linux")]
    linux::futex_wait(atomic, expected);

    #[cfg(target_os = "macos")]
    macos::futex_wait(atomic, expected);
}

pub fn futex_wake(atomic: &AtomicU32) {
    #[cfg(target_os = "linux")]
    linux::futex_wake(atomic);

    #[cfg(target_os = "macos")]
    macos::futex_wake(atomic);
}

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::ptr;

    pub fn futex_wait(atomic: &AtomicU32, expected: u32) {
        if atomic.load(Ordering::Relaxed) != expected {
            return;
        }
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                atomic as *const AtomicU32 as *const u32,
                libc::FUTEX_WAIT,
                expected,
                ptr::null::<libc::timespec>(),
                ptr::null::<u32>(),
                0u32,
            );
        }
    }

    pub fn futex_wake(atomic: &AtomicU32) {
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                atomic as *const AtomicU32 as *const u32,
                libc::FUTEX_WAKE,
                1,
                std::ptr::null::<libc::timespec>(),
                std::ptr::null::<u32>(),
                0u32,
            );
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::atomic::{AtomicU32, Ordering};

    const UL_COMPARE_AND_WAIT: u32 = 1;

    extern "C" {
        fn __ulock_wait(op: u32, addr: *const std::ffi::c_void, val: u64, timeout: u32) -> i32;
        fn __ulock_wake(op: u32, addr: *const std::ffi::c_void, val: u64) -> i32;
    }

    pub fn futex_wait(atomic: &AtomicU32, expected: u32) {
        if atomic.load(Ordering::Relaxed) != expected {
            return;
        }
        unsafe {
            __ulock_wait(
                UL_COMPARE_AND_WAIT,
                atomic as *const AtomicU32 as *const std::ffi::c_void,
                expected as u64,
                0,
            );
        }
    }

    pub fn futex_wake(atomic: &AtomicU32) {
        unsafe {
            __ulock_wake(
                UL_COMPARE_AND_WAIT,
                atomic as *const AtomicU32 as *const std::ffi::c_void,
                0,
            );
        }
    }
}