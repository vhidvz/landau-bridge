//! One running app per data folder (plan 130 R-m). An exclusive lock on `bridge.lock` beside the
//! database, taken before the database is opened: a second instance would otherwise mark the first
//! one's running job INTERRUPTED at its own start-up, and two processes could each write a shelf.
//! The operating system releases the lock when the process ends, however it ends.

use std::fs::{File, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::errors::{BridgeError, Result};

pub const LOCK_FILE: &str = "bridge.lock";

/// Held for the life of the app; dropping it releases the folder.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

/// How long a starting app waits for the folder before calling itself a second instance. A restart
/// after an update starts the new copy and only then ends the old one (Tauri's `restart()`), so the
/// first holder may still be on its way out (slice 2's review).
pub const START_WAIT: Duration = Duration::from_secs(5);

/// [`acquire`], retried for up to `wait` while another process holds the folder.
pub fn acquire_waiting(dir: &Path, wait: Duration) -> Result<InstanceLock> {
    let started = Instant::now();
    loop {
        match acquire(dir) {
            Err(BridgeError::AlreadyRunning) if started.elapsed() < wait => {
                std::thread::sleep(Duration::from_millis(100))
            }
            other => return other,
        }
    }
}

/// Takes the folder's lock, or [`BridgeError::AlreadyRunning`] when another process holds it.
pub fn acquire(dir: &Path) -> Result<InstanceLock> {
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK_FILE))
        .map_err(|e| BridgeError::Other(format!("lock file: {e}")))?;
    match file.try_lock() {
        Ok(()) => Ok(InstanceLock { _file: file }),
        Err(TryLockError::WouldBlock) => Err(BridgeError::AlreadyRunning),
        Err(TryLockError::Error(e)) => Err(BridgeError::Other(format!("lock file: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lock is per open file, so a second open in this process stands in for a second app.
    #[test]
    fn a_second_instance_cannot_start_until_the_first_ends() {
        let dir = tempfile::tempdir().unwrap();
        let first = acquire(dir.path()).unwrap();
        assert_eq!(
            acquire(dir.path()).unwrap_err(),
            BridgeError::AlreadyRunning
        );
        assert_eq!(acquire(dir.path()).unwrap_err().kind(), "AlreadyRunning");
        drop(first);
        assert!(
            acquire(dir.path()).is_ok(),
            "free again once the first is gone"
        );
    }

    #[test]
    fn another_data_folder_is_another_instance() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let _one = acquire(a.path()).unwrap();
        assert!(acquire(b.path()).is_ok());
    }

    /// A restart's new copy waits for the old one to let go; a real second copy is still refused.
    #[test]
    fn a_starting_copy_waits_for_the_one_on_its_way_out() {
        let dir = tempfile::tempdir().unwrap();
        let old = acquire(dir.path()).unwrap();
        let leaving = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(old);
        });
        let new = acquire_waiting(dir.path(), Duration::from_secs(3)).unwrap();
        leaving.join().unwrap();
        assert_eq!(
            acquire(dir.path()).unwrap_err(),
            BridgeError::AlreadyRunning,
            "the new copy holds it"
        );
        drop(new);

        let other = tempfile::tempdir().unwrap();
        let _first = acquire(other.path()).unwrap();
        let started = Instant::now();
        assert_eq!(
            acquire_waiting(other.path(), Duration::from_millis(400)).unwrap_err(),
            BridgeError::AlreadyRunning
        );
        assert!(started.elapsed() >= Duration::from_millis(400));
    }
}
