//! # Shared Memory Module
//!
//! This module provides a cross-platform abstraction for shared memory using memory-mapped files.
//! It's designed for high-performance inter-process communication (IPC).

#![warn(missing_docs)]

use std::fmt::Debug;
use std::io;
#[cfg(unix)]
use std::os::fd::RawFd;

/// Defines the interface for shared memory backends across different platforms.
pub trait SharedMemoryBackend: Send + Sync + Debug {
    /// Returns a raw pointer to the start of the shared memory region.
    fn as_ptr(&self) -> *mut u8;

    /// Returns the size of the shared memory region in bytes.
    fn size(&self) -> usize;

    /// Returns a platform-specific handle to the shared memory.
    fn raw_handle(&self) -> RawHandle;
}

/// Platform-specific handle type
#[derive(Debug, Clone, Copy)]
pub enum RawHandle {
    /// Unix file descriptor (Linux)
    Fd(RawFd),
}

/// Creates a new shared memory region with the specified size.
#[cfg(target_os = "linux")]
pub fn create_shared_memory(
    size: usize,
    name: Option<&str>,
) -> io::Result<Box<dyn SharedMemoryBackend>> {
    Ok(Box::new(linux::LinuxSharedMemory::create(size, name)?))
}

/// Attaches to an existing shared memory region.
#[cfg(target_os = "linux")]
pub fn attach_shared_memory(name: &str, size: usize) -> io::Result<Box<dyn SharedMemoryBackend>> {
    Ok(Box::new(linux::LinuxSharedMemory::attach(name, size)?))
}

#[cfg(not(target_os = "linux"))]
pub fn create_shared_memory(
    _size: usize,
    _name: Option<&str>,
) -> io::Result<Box<dyn SharedMemoryBackend>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Shared memory only supported on Linux",
    ))
}

#[cfg(not(target_os = "linux"))]
pub fn attach_shared_memory(_name: &str, _size: usize) -> io::Result<Box<dyn SharedMemoryBackend>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Shared memory only supported on Linux",
    ))
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::ffi::CString;
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::{AsRawFd, IntoRawFd};
    use std::ptr::{self, NonNull};

    #[derive(Debug)]
    pub struct LinuxSharedMemory {
        ptr: NonNull<u8>,
        size: usize,
        fd: RawFd,
        // Stores the original pointer and size if we had to align it manually
        original_ptr: Option<(*mut u8, usize)>,
    }

    unsafe impl Send for LinuxSharedMemory {}
    unsafe impl Sync for LinuxSharedMemory {}

    impl LinuxSharedMemory {
        pub fn create(size: usize, name: Option<&str>) -> io::Result<Self> {
            // Use memfd_create if name is None, otherwise use /dev/shm file
            if let Some(name) = name {
                Self::create_file_backed(size, name)
            } else {
                Self::create_anonymous(size)
            }
        }

        fn create_anonymous(size: usize) -> io::Result<Self> {
            let name = CString::new("dmxp_shm").unwrap();
            let fd = unsafe { libc::memfd_create(name.as_ptr(), 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            if unsafe { libc::ftruncate(fd, size as i64) } != 0 {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }

            Self::mmap(fd, size)
        }

        fn create_file_backed(size: usize, name: &str) -> io::Result<Self> {
            let path = format!("/dev/shm/{}", name);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to create shared memory file at {}: {}", path, e),
                    )
                })?;

            let fd = file.as_raw_fd();
            if unsafe { libc::ftruncate(fd, size as i64) } != 0 {
                return Err(io::Error::last_os_error());
            }

            // We need to keep the fd open, so we convert it to raw and forget the File
            let fd = file.into_raw_fd();
            Self::mmap(fd, size)
        }

        pub fn attach(name: &str, expected_size: usize) -> io::Result<Self> {
            let path = format!("/dev/shm/{}", name);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Failed to open shared memory at {}: {}", path, e),
                    )
                })?;

            let size = file.metadata()?.len() as usize;
            if size < expected_size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Shared memory size too small: expected at least {}, got {}",
                        expected_size, size
                    ),
                ));
            }

            let fd = file.into_raw_fd();
            Self::mmap(fd, size)
        }

        fn mmap(fd: RawFd, size: usize) -> io::Result<Self> {
            // We map slightly more to ensure we can align to 128 bytes if needed
            // But mmap usually returns page-aligned memory (4096 bytes), which is > 128.
            // So we just map the size.
            // Wait, the original code added 127 bytes. Let's stick to that for safety.
            let map_size = size + 127;

            let ptr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    map_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    0,
                )
            };

            if ptr == libc::MAP_FAILED {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }

            let aligned_ptr = ((ptr as usize + 127) & !127) as *mut u8;

            Ok(Self {
                ptr: NonNull::new(aligned_ptr).unwrap(),
                size,
                fd,
                original_ptr: Some((ptr as *mut u8, map_size)),
            })
        }
    }

    impl Drop for LinuxSharedMemory {
        fn drop(&mut self) {
            unsafe {
                if let Some((ptr, size)) = self.original_ptr {
                    libc::munmap(ptr as *mut libc::c_void, size);
                }
                libc::close(self.fd);
            }
        }
    }

    impl SharedMemoryBackend for LinuxSharedMemory {
        fn as_ptr(&self) -> *mut u8 {
            self.ptr.as_ptr()
        }

        fn size(&self) -> usize {
            self.size
        }

        fn raw_handle(&self) -> RawHandle {
            RawHandle::Fd(self.fd)
        }
    }
}
