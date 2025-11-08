// Shared memory backend abstraction for Linux
// Uses memfd_create + mmap for efficient shared memory

use std::io;
use std::ptr;
use std::ptr::NonNull;
use std::fmt::Debug;
// #[cfg(unix)]
use std::os::fd::AsRawFd;
// #[cfg(unix)]
use std::os::fd::IntoRawFd;

/// Shared memory backend trait for cross-platform memory mapping
pub trait SharedMemoryBackend: Send + Sync + Debug {
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
// #[cfg(target_os = "linux")]
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
// #[cfg(target_os = "linux")]
pub fn attach_shared_memory(name: &str, size: usize) -> io::Result<Box<dyn SharedMemoryBackend>> {
    // LinuxSharedMemory::attach(name, size).map(|shm| Box::new(shm) as Box<dyn SharedMemoryBackend>)
            // First try to open the file in /dev/shm
        let path = format!("/dev/shm/{}", name);
        if let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            // Memory-mapped file approach
            let size = file.metadata()?.len() as usize;
            // Allocate extra space to ensure we can align to 128 bytes
            let mapping = unsafe {
                let total_size = size + 127; // Extra space for alignment
                let ptr = libc::mmap(
                    std::ptr::null_mut(),
                    total_size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    file.as_raw_fd(),
                    0,
                );
                
                if ptr == libc::MAP_FAILED {
                    return Err(io::Error::last_os_error());
                }
                
                // Align the pointer to 128 bytes
                let aligned_ptr = ((ptr as usize + 127) & !127) as *mut libc::c_void;
                
                // Store the original pointer for munmap
                let orig_ptr = ptr;
                
                // Return both the aligned pointer and the original pointer
                (aligned_ptr as *mut u8, orig_ptr, total_size)
            };

            let (aligned_ptr, orig_ptr, total_size) = mapping;
            
            Ok(Box::new(LinuxSharedMemory {
                ptr: unsafe { std::ptr::NonNull::new(aligned_ptr).unwrap() },
                size,
                fd: file.into_raw_fd(),
                original_ptr: Some((orig_ptr as *mut u8, total_size)),
            }))
        }

        // Fall back to memfd if file not found
        LinuxSharedMemory::attach(name, size).map(|shm| Box::new(shm) as Box<dyn SharedMemoryBackend>)
}

// #[cfg(target_os = "linux")]
impl LinuxSharedMemory {
    pub fn attach(name: &str, _size: usize) -> io::Result<Self> {
        // Try to open an existing memfd
        let c_name = std::ffi::CString::new(name).unwrap();
        let fd = unsafe { libc::memfd_create(c_name.as_ptr(), 0) };
        
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // Get the size from the existing memfd
        let size = unsafe {
            let mut stat = std::mem::zeroed();
            if libc::fstat(fd, &mut stat) != 0 {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }
            stat.st_size as usize
        };

        // Allocate extra space to ensure we can align to 128 bytes
        let (ptr, original_ptr) = unsafe {
            let total_size = size + 127; // Extra space for alignment
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                total_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if ptr == libc::MAP_FAILED {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }
            
            // Align the pointer to 128 bytes
            let aligned_ptr = ((ptr as usize + 127) & !127) as *mut u8;
            
            // Return both the aligned pointer and the original pointer
            (aligned_ptr, Some((ptr as *mut u8, total_size)))
        };

        Ok(Self {
            ptr: unsafe { std::ptr::NonNull::new(ptr).unwrap() },
            size,
            fd,
            original_ptr: original_ptr,
        })
    }
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

// #[cfg(target_os = "linux")]
use libc::{c_void, syscall, SYS_memfd_create};
// #[cfg(target_os = "linux")]
use std::ffi::CString;
// #[cfg(target_os = "linux")]
use std::os::unix::io::RawFd;

// #[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct LinuxSharedMemory {
    ptr: NonNull<u8>,
    size: usize,
    fd: i32,
    original_ptr: Option<(*mut u8, usize)>,
}

// #[cfg(target_os = "linux")]
unsafe impl Send for LinuxSharedMemory {}
// #[cfg(target_os = "linux")]
unsafe impl Sync for LinuxSharedMemory {}

// #[cfg(target_os = "linux")]
impl LinuxSharedMemory {
    /// Create a new shared memory region using memfd_create
    pub fn create(size: usize, name: Option<&str>) -> io::Result<Self> {
        // Use memfd_create on Linux via syscall
        let c_name = CString::new(name.unwrap_or("dmxp_shm")).unwrap();
        let flags = 0u64; // MFD_CLOEXEC would be 1
        
        let fd = unsafe {
            syscall(SYS_memfd_create, c_name.as_ptr(), flags) as i32
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
        let (ptr, original_ptr) = unsafe {
            let total_size = size + 127; // Extra space for alignment
            let ptr = libc::mmap(
                ptr::null_mut(),
                total_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if ptr == ptr::null_mut() {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(err);
            }
            
            // Align the pointer to 128 bytes
            let aligned_ptr = ((ptr as usize + 127) & !127) as *mut u8;
            
            // Return both the aligned pointer and the original pointer
            (aligned_ptr, Some((ptr as *mut u8, total_size)))
        };

        Ok(Self {
            ptr: NonNull::new(ptr).unwrap(),
            size,
            fd,
            original_ptr: original_ptr,
        })
    }
}

// #[cfg(target_os = "linux")]
impl Drop for LinuxSharedMemory {
    fn drop(&mut self) {
        unsafe {
            // Use the original pointer and size for munmap
            if let Some((ptr, size)) = self.original_ptr {
                libc::munmap(ptr as *mut libc::c_void, size);
            } else {
                libc::munmap(self.ptr.as_ptr() as *mut libc::c_void, self.size);
            }
            libc::close(self.fd);
        }
    }
}

// #[cfg(target_os = "linux")]
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
