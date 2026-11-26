//! Proof that the window that wrote a UI link file still runs (#3995).
//!
//! The token in a discovery file proves nothing about the peer that
//! answers: whoever reads a stale file can compute every token proof, and
//! a false server can take the freed port of a crashed window. So a
//! window holds an exclusive OS file lock on `<pid>.lock` in the link
//! folder for its whole life. The OS releases the lock when the process
//! ends, a crash included, so the lock and not the pid is the proof: a
//! reused pid cannot hold it.
//!
//! The CLI uses an entry only while the lock is held, before the hello
//! and again after it. A file whose lock is free is stale: it is removed
//! and logged. A file with no lock to test is unproved: it is skipped and
//! logged, never dialed.
//!
//! Provenance: our own design (docs/decisions/3995-ui-link-owner-lock.md)
//! over std's advisory file locks (`File::try_lock`: `flock` on Unix,
//! `LockFileEx` on Windows). Clean-room, public sources only.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

/// The lock file of the window with this process id.
#[must_use]
pub fn lock_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.lock"))
}

/// The window's end: take the lock of `pid` and keep it while the returned
/// file stays open.
///
/// # Errors
/// The lock file cannot be opened, or another live process holds it.
pub(crate) fn hold(dir: &Path, pid: u32) -> std::io::Result<File> {
    let path = lock_path(dir, pid);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Error::new(
            ErrorKind::AlreadyExists,
            format!("{} is held by another running process", path.display()),
        )),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// What the lock says about the window that wrote a link file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// A live process holds the lock: the window runs.
    Alive,
    /// The lock is free: the window has ended and the file is stale.
    Gone,
    /// No lock can be tested; the reason, for the diagnostic.
    Unproved(String),
}

/// Test the lock of `pid` in `dir` without holding it.
#[must_use]
pub fn owner(dir: &Path, pid: u32) -> Owner {
    let path = lock_path(dir, pid);
    let file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Owner::Unproved(format!(
                "no lock file {} (a window of an older build, or a file no window wrote)",
                path.display()
            ));
        }
        Err(e) => return Owner::Unproved(format!("cannot open {}: {e}", path.display())),
    };
    match file.try_lock() {
        Err(TryLockError::WouldBlock) => Owner::Alive,
        // Release the lock at once, never by the close of `file` (#4041):
        // on Unix a child that another thread starts holds a copy of the
        // descriptor until its exec, and a lock left held by the close
        // makes the next test read the ended window as alive.
        Ok(()) => match file.unlock() {
            Ok(()) => Owner::Gone,
            Err(e) => Owner::Unproved(format!("cannot release the lock {}: {e}", path.display())),
        },
        Err(TryLockError::Error(e)) => Owner::Unproved(format!("cannot test the lock {}: {e}", path.display())),
    }
}

/// Remove the files of a window that has ended: its link file, socket
/// file and lock file. Only these names in `dir`; never a path read from
/// the link file. A failed removal is logged.
pub fn remove_stale(dir: &Path, pid: u32) {
    for name in [format!("{pid}.json"), format!("{pid}.sock"), format!("{pid}.lock")] {
        let path = dir.join(name);
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != ErrorKind::NotFound
        {
            tracing::warn!("ui link: cannot remove the stale file {}: {e}", path.display());
        }
    }
}
