use std::io;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

// Test lock to prevent parallel test execution
static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::const_mutex(());

#[test]
fn test_mpmc_integration() -> io::Result<()> {
    let _guard = TEST_LOCK.lock();

    // Clean up any existing shared memory
    cleanup_shared_memory();

    // Number of messages to send
    const NUM_MESSAGES: usize = 1000;

    // Start producer process FIRST (it creates the shared memory)
    let producer = Command::new("cargo")
        .arg("run")
        .arg("--example")
        .arg("producer")
        .arg(NUM_MESSAGES.to_string())
        .arg("--auto-exit")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Give producer time to create shared memory
    thread::sleep(Duration::from_millis(500));

    // Start consumer process (it attaches to existing shared memory)
    let consumer = Command::new("cargo")
        .arg("run")
        .arg("--example")
        .arg("consumer")
        .arg(NUM_MESSAGES.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for consumer to finish first (it should receive all messages)
    let consumer_output = consumer.wait_with_output()?;

    // Then wait for producer to finish
    let producer_output = producer.wait_with_output()?;

    // Check both succeeded
    if !producer_output.status.success() {
        eprintln!(
            "Producer stderr: {}",
            String::from_utf8_lossy(&producer_output.stderr)
        );
        panic!("Producer failed");
    }

    if !consumer_output.status.success() {
        eprintln!(
            "Consumer stderr: {}",
            String::from_utf8_lossy(&consumer_output.stderr)
        );
        panic!("Consumer failed");
    }

    // Verify consumer output
    let consumer_stdout = String::from_utf8_lossy(&consumer_output.stdout);
    assert!(
        consumer_stdout.contains("All messages received successfully"),
        "Consumer did not receive all messages"
    );

    Ok(())
}

// Clean up shared memory
fn cleanup_shared_memory() {
    // Implementation depends on your platform
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        use std::path::Path;

        let shm_dir = Path::new("/dev/shm");
        if let Ok(entries) = fs::read_dir(shm_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("dmxp") {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_shared_memory() {
        // Just verify the cleanup function doesn't panic
        cleanup_shared_memory();
    }
}
