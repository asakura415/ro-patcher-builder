use std::fs::{File, OpenOptions};
use std::path::Path;
use anyhow::{bail, Context, Result};
use fs2::FileExt;

pub struct FileLock {
    _file: File,
}

impl FileLock {
    /// Attempts to acquire an exclusive lock on the specified file path.
    /// If another process is holding the lock, returns an error.
    pub fn try_acquire(lock_path: impl AsRef<Path>) -> Result<Self> {
        let lock_path = lock_path.as_ref();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .with_context(|| format!("Failed to open lock file at {:?}", lock_path))?;

        match file.try_lock_exclusive() {
            Ok(()) => Ok(Self { _file: file }),
            Err(_) => {
                bail!(
                    "Another instance of the patcher is currently running or updating the game files."
                );
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_lock() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let lock1 = FileLock::try_acquire(path);
        assert!(lock1.is_ok());

        let lock2 = FileLock::try_acquire(path);
        assert!(lock2.is_err());

        drop(lock1);

        let lock3 = FileLock::try_acquire(path);
        assert!(lock3.is_ok());
    }
}
