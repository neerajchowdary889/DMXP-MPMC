// Shared memory backend abstraction for Linux
// Uses memfd_create + mmap for efficient shared memory

use std::io;
use std::ptr;
use std::ptr::NonNull;
use std::fmt::Debug;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::fd::IntoRawFd;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;

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
    // Align the size to 128 bytes
    let aligned_size = (size + 127) & !127;
    
    // Try to open the file in /dev/shm
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

    let file_size = file.metadata()?.len() as usize;
    if file_size < aligned_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Shared memory size too small: expected at least {} bytes, got {}",
                aligned_size, file_size
            ),
        ));
    }

    // Memory-mapped file approach
    let (aligned_ptr, orig_ptr, total_size) = unsafe {
        let total_size = file_size + 127; // Extra space for alignment
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
        let aligned_ptr = ((ptr as usize + 127) & !127) as *mut u8;
        
        (aligned_ptr, ptr as *mut u8, total_size)
    };
    
    Ok(Box::new(LinuxSharedMemory {
        ptr: NonNull::new(aligned_ptr).unwrap(),
        size: file_size,
        fd: file.into_raw_fd(),
        original_ptr: Some((orig_ptr, total_size)),
    }))
}

#[cfg(target_os = "linux")]
impl LinuxSharedMemory {
    pub fn attach(name: &str, expected_size: usize) -> io::Result<Self> {
        // Try to open an existing memfd
        let c_name = std::ffi::CString::new(name).unwrap();
        let fd = unsafe { libc::memfd_create(c_name.as_ptr(), 0) };
        
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // Get the size from the existing memfd
        let actual_size = unsafe {
            let mut stat = std::mem::zeroed();
            if libc::fstat(fd, &mut stat) != 0 {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }
            stat.st_size as usize
        };

        // Verify the size matches expected
        if actual_size < expected_size {
            unsafe { libc::close(fd) };
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Shared memory too small: expected at least {}, got {}", 
                    expected_size, actual_size),
            ));
        }

        // Rest of the function remains the same...
        let (ptr, original_ptr) = unsafe {
            // Use the actual size from the memfd
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                actual_size,  // Use actual_size instead of size
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if ptr == libc::MAP_FAILED {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }
            
            // Align the pointer to 128 bytes
            let aligned_ptr = ((ptr as usize + 127) & !127) as *mut u8;
            
            // Return both the aligned pointer and the original pointer
            (aligned_ptr, Some((ptr as *mut u8, actual_size)))
        };

        Ok(Self {
            ptr: std::ptr::NonNull::new(ptr).unwrap(),
            size: actual_size,  // Store actual size
            fd,
            original_ptr,
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

#[cfg(target_os = "linux")]
use libc::{c_void, syscall, SYS_memfd_create};
#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::os::unix::io::RawFd;

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct LinuxSharedMemory {
    ptr: NonNull<u8>,
    size: usize,
    fd: i32,
    original_ptr: Option<(*mut u8, usize)>,
}

#[cfg(target_os = "linux")]
unsafe impl Send for LinuxSharedMemory {}
#[cfg(target_os = "linux")]
unsafe impl Sync for LinuxSharedMemory {}

#[cfg(target_os = "linux")]
impl LinuxSharedMemory {
    /// Create a new shared memory region using /dev/shm
    pub fn create(size: usize, name: Option<&str>) -> io::Result<Self> {
        let shm_name = name.unwrap_or("dmxp_shm");
        let path = format!("/dev/shm/{}", shm_name);
        
        // Create or truncate the file in /dev/shm
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

        // Set size
        if unsafe { libc::ftruncate(fd, size as i64) } != 0 {
            return Err(io::Error::last_os_error());
        }

        // Keep the file descriptor alive
        let fd = file.into_raw_fd();

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

            if ptr == libc::MAP_FAILED {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }
            
            // Align the pointer to 128 bytes
            let aligned_ptr = ((ptr as usize + 127) & !127) as *mut u8;
            
            (aligned_ptr, Some((ptr as *mut u8, total_size)))
        };

        Ok(Self {
            ptr: NonNull::new(ptr).unwrap(),
            size,
            fd,
            original_ptr,
        })
    }
}

#[cfg(target_os = "linux")]
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