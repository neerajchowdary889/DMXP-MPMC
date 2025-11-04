// Shared memory backend tests for Linux
// Run with: cargo test --test shared_memory -- --nocapture

#[cfg(target_os = "linux")]
mod linux_tests {
    use dmxp_kvcache::Core::{create_shared_memory, attach_shared_memory, SharedMemoryBackend};

    #[test]
    fn test_create_shared_memory() {
        let size = 4096;
        let shm = create_shared_memory(size, Some("test_create")).unwrap();
        
        assert_eq!(shm.size(), size);
        assert!(!shm.as_ptr().is_null());
        
        // Test writing to the memory
        unsafe {
            let slice = std::slice::from_raw_parts_mut(shm.as_ptr(), size);
            slice[0] = 0x42;
            assert_eq!(slice[0], 0x42);
        }
    }

    #[test]
    fn test_shared_memory_size() {
        let sizes = vec![1024, 4096, 65536, 1024 * 1024];
        
        for size in sizes {
            let shm = create_shared_memory(size, None).unwrap();
            assert_eq!(shm.size(), size);
        }
    }

    #[test]
    fn test_shared_memory_read_write() {
        let size = 8192;
        let shm = create_shared_memory(size, Some("test_rw")).unwrap();
        
        unsafe {
            let slice = std::slice::from_raw_parts_mut(shm.as_ptr(), size);
            
            // Write test pattern
            for i in 0..100 {
                slice[i] = (i % 256) as u8;
            }
            
            // Read back
            for i in 0..100 {
                assert_eq!(slice[i], (i % 256) as u8);
            }
        }
    }

    #[test]
    fn test_raw_handle() {
        let shm = create_shared_memory(4096, Some("test_handle")).unwrap();
        let handle = shm.raw_handle();
        
        match handle {
            dmxp_kvcache::Core::RawHandle::Fd(fd) => {
                assert!(fd > 0, "File descriptor should be positive");
            }
        }
    }

    #[test]
    fn test_attach_not_implemented() {
        // attach_shared_memory is not yet implemented for memfd_create
        let result = attach_shared_memory("test_attach", 4096);
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
    }

    #[test]
    fn test_mmap_write_persistence() {
        // Verify that writes to the mmap'd region persist
        let size = 4096;
        let shm = create_shared_memory(size, Some("test_persistence")).unwrap();
        
        unsafe {
            let slice = std::slice::from_raw_parts_mut(shm.as_ptr(), size);
            
            // Write a pattern
            for i in 0..size {
                slice[i] = (i % 256) as u8;
            }
            
            // Verify the pattern is still there (mmap'd memory persists)
            for i in 0..size {
                assert_eq!(slice[i], (i % 256) as u8, "Write to mmap'd region should persist");
            }
            
            // Write a different pattern and verify
            slice[0] = 0xFF;
            slice[1000] = 0xAA;
            assert_eq!(slice[0], 0xFF);
            assert_eq!(slice[1000], 0xAA);
        }
    }

    #[test]
    fn test_mmap_zero_initialized() {
        // Verify mmap'd memory is zero-initialized
        let size = 1024;
        let shm = create_shared_memory(size, Some("test_zero")).unwrap();
        
        unsafe {
            let slice = std::slice::from_raw_parts_mut(shm.as_ptr(), size);
            for i in 0..size {
                assert_eq!(slice[i], 0, "Mmap'd memory should be zero-initialized");
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod non_linux_tests {
    use dmxp_kvcache::Core::{create_shared_memory, attach_shared_memory};

    #[test]
    fn test_unsupported_platform() {
        let result = create_shared_memory(4096, None);
        assert!(result.is_err());
        
        if let Err(err) = result {
            assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
        }
    }

    #[test]
    fn test_attach_unsupported_platform() {
        let result = attach_shared_memory("test", 4096);
        assert!(result.is_err());
        
        if let Err(err) = result {
            assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
        }
    }
}

