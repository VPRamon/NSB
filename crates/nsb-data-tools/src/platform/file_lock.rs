//! Advisory exclusive file locks for multi-process bulk coordination.

#![allow(unsafe_code)]

use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// Held exclusive flock; unlocked on drop.
#[derive(Debug)]
pub struct ExclusiveFileLock {
    path: PathBuf,
    file: File,
}

impl ExclusiveFileLock {
    /// Path of the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ExclusiveFileLock {
    fn drop(&mut self) {
        let _ = flock_unlock(&self.file);
    }
}

/// Create (or open) `path` and take an exclusive advisory lock, blocking.
pub fn lock_exclusive(path: &Path) -> Result<ExclusiveFileLock> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create lock parent {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("open lock file {}", path.display()))?;
    flock_exclusive_blocking(&file)
        .with_context(|| format!("flock exclusive {}", path.display()))?;
    Ok(ExclusiveFileLock {
        path: path.to_path_buf(),
        file,
    })
}

/// Try to take an exclusive lock without blocking. Returns `None` if held elsewhere.
pub fn try_lock_exclusive(path: &Path) -> Result<Option<ExclusiveFileLock>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create lock parent {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("open lock file {}", path.display()))?;
    match flock_exclusive_nonblocking(&file) {
        Ok(()) => Ok(Some(ExclusiveFileLock {
            path: path.to_path_buf(),
            file,
        })),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(err).with_context(|| format!("flock try exclusive {}", path.display())),
    }
}

fn flock_exclusive_blocking(file: &File) -> std::io::Result<()> {
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn flock_exclusive_nonblocking(file: &File) -> std::io::Result<()> {
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn flock_unlock(file: &File) -> std::io::Result<()> {
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Require that a lock acquisition succeeded (helper for call sites).
#[allow(dead_code)]
pub fn expect_lock(lock: Option<ExclusiveFileLock>, path: &Path) -> Result<ExclusiveFileLock> {
    lock.ok_or_else(|| anyhow::anyhow!("could not acquire lock at {}", path.display()))
}

/// Fail if path cannot be locked exclusively without waiting (debug helper).
#[allow(dead_code)]
pub fn ensure_unlocked_or_bail(path: &Path) -> Result<()> {
    match try_lock_exclusive(path)? {
        Some(_lock) => Ok(()),
        None => bail!("lock already held: {}", path.display()),
    }
}
