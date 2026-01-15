use std::sync::atomic::AtomicU32;

#[cfg(target_os = "linux")]
pub fn futex_wait(atomic: &AtomicU32, expected: u32) {
    use std::ptr;
    use std::sync::atomic::Ordering;

    // Check condition first to avoid syscall if possible
    if atomic.load(Ordering::Relaxed) != expected {
        return;
    }

    unsafe {
        libc::syscall(
            libc::SYS_futex,
            atomic as *const AtomicU32 as *const u32,
            libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
            expected,
            ptr::null::<libc::timespec>(),
            ptr::null::<u32>(),
            0u32,
        );
    }
}

#[cfg(target_os = "linux")]
pub fn futex_wake(atomic: &AtomicU32) {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            atomic as *const AtomicU32 as *const u32,
            libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
            1, // Wake 1 waiter
            std::ptr::null::<libc::timespec>(),
            std::ptr::null::<u32>(),
            0u32,
        );
    }
}

#[cfg(not(target_os = "linux"))]
pub fn futex_wait(_atomic: &AtomicU32, _expected: u32) {
    // Fallback for non-Linux: busy wait with yield
    std::thread::yield_now();
}

#[cfg(not(target_os = "linux"))]
pub fn futex_wake(_atomic: &AtomicU32) {
    // No-op on non-Linux
}
