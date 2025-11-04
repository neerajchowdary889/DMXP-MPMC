// Shared memory backend abstraction for Linux
// Uses memfd_create + mmap for efficient shared memory

use std::io;
use std::ptr::NonNull;

/// Shared memory backend trait for cross-platform memory mapping
pub trait SharedMemoryBackend: Send + Sync {
    /// Get a pointer to the mapped memory region
    fn as_ptr(&self) -> *mut u8;
    
    /// Get the size of the mapped region in bytes
    fn size(&self) -> usize;
    
    /// Get the underlying file descriptor
    fn raw_handle(&self) -> RawHandle;
}

/// Platform-specific handle type
#[derive(Debug, Clone, Copy)]
pub enum RawHandle {
    /// Unix file descriptor (Linux)
    Fd(i32),
}

/// Create a new shared memory region with the specified size
/// 
/// # Arguments
/// * `size` - Size of the shared memory region in bytes
/// * `name` - Optional name for the shared memory region (for cross-process access)
/// 
/// # Returns
/// A boxed trait object implementing SharedMemoryBackend
#[cfg(target_os = "linux")]
pub fn create_shared_memory(size: usize, name: Option<&str>) -> io::Result<Box<dyn SharedMemoryBackend>> {
    Ok(Box::new(LinuxSharedMemory::create(size, name)?))
}

/// Attach to an existing shared memory region
/// Note: For memfd_create, cross-process attachment requires file descriptor passing
/// via /proc/self/fd/ or similar mechanisms. This is a simplified implementation.
/// 
/// # Arguments
/// * `name` - Name of the shared memory region to attach to
/// * `size` - Expected size of the region (for validation)
/// 
/// # Returns
/// A boxed trait object implementing SharedMemoryBackend
#[cfg(target_os = "linux")]
pub fn attach_shared_memory(name: &str, size: usize) -> io::Result<Box<dyn SharedMemoryBackend>> {
    LinuxSharedMemory::attach(name, size).map(|shm| Box::new(shm) as Box<dyn SharedMemoryBackend>)
}

#[cfg(not(target_os = "linux"))]
pub fn create_shared_memory(_size: usize, _name: Option<&str>) -> io::Result<Box<dyn SharedMemoryBackend>> {
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
use libc::{c_void, syscall, SYS_memfd_create};
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::os::unix::io::RawFd;

#[cfg(target_os = "linux")]
pub struct LinuxSharedMemory {
    ptr: NonNull<u8>,
    size: usize,
    fd: RawFd,
}

#[cfg(target_os = "linux")]
unsafe impl Send for LinuxSharedMemory {}
#[cfg(target_os = "linux")]
unsafe impl Sync for LinuxSharedMemory {}

#[cfg(target_os = "linux")]
impl LinuxSharedMemory {
    /// Create a new shared memory region using memfd_create
    pub fn create(size: usize, name: Option<&str>) -> io::Result<Self> {
        // Use memfd_create on Linux via syscall
        let c_name = CString::new(name.unwrap_or("dmxp_shm")).unwrap();
        let flags = 0u64; // MFD_CLOEXEC would be 1
        
        let fd = unsafe {
            syscall(SYS_memfd_create, c_name.as_ptr(), flags) as RawFd
        };

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // Set size
        if unsafe { libc::ftruncate(fd, size as i64) } != 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }

        // Map memory
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            ) as *mut u8
        };

        if ptr == ptr::null_mut() {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }

        Ok(Self {
            ptr: NonNull::new(ptr).unwrap(),
            size,
            fd,
        })
    }

    /// Attach to an existing shared memory region
    /// Note: This is a placeholder - memfd_create requires file descriptor passing
    /// for cross-process access (e.g., via /proc/self/fd/ or Unix domain sockets)
    pub fn attach(_name: &str, _size: usize) -> io::Result<Self> {
        // For memfd_create, cross-process attachment requires file descriptor passing
        // via /proc/self/fd/ or similar mechanisms
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Attach not yet implemented for Linux memfd. Use file descriptor passing via /proc/self/fd/",
        ))
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxSharedMemory {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr() as *mut c_void, self.size);
            libc::close(self.fd);
        }
    }
}

#[cfg(target_os = "linux")]
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
