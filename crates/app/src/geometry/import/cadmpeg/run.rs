//! The bounded child process and the private folder of the cadmpeg runner
//! (#1873). The child's standard output and error go to files in the
//! private folder, never to pipes: no reader can block on a process that
//! holds a pipe open, and the size of each file is a limit the loop reads.
//!
//! Provenance: our own design over `std::process`; no cadmpeg code.

use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The most that a child may print on standard output or standard error.
pub(super) const CAPTURE_MAX: u64 = 16 << 20;
/// How often the loop looks at the child, the clock and the file sizes.
const POLL: Duration = Duration::from_millis(15);

/// A private folder that is removed when this value is dropped: on
/// success, on an error return and on a panic.
pub(super) struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    /// Make a new folder under `root`. The name is unique to this process
    /// and call. `create` fails on a name that exists, so a folder that
    /// another process made first is never adopted.
    pub(super) fn new(root: &Path) -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut last = std::io::Error::other("no private folder name was free");
        for _ in 0..16 {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            let dir = root.join(format!("camcraft-cadmpeg-{}-{nanos:08x}-{n}", std::process::id()));
            match private_dir(&dir) {
                Ok(()) => return Ok(Self { dir }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    pub(super) fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            tracing::warn!("cadmpeg: the private folder {} was not removed: {e}", self.dir.display());
        }
    }
}

/// Create `dir` for this user only. It must not exist.
fn private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::DirBuilder::new().create(dir)
    }
}

/// What a finished child left.
pub(super) struct Output {
    /// `None` when a signal ended the child.
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Why a run gave no [`Output`].
#[derive(Debug)]
pub(super) enum RunError {
    /// The program did not start, or a capture file failed.
    Io(std::io::Error),
    /// The time limit ended first; the child was stopped.
    Timeout,
    /// The child printed more than [`CAPTURE_MAX`]; it was stopped.
    Flood,
    /// The watched output file grew past its limit; the child was stopped.
    Oversize,
}

impl From<std::io::Error> for RunError {
    fn from(e: std::io::Error) -> Self {
        RunError::Io(e)
    }
}

/// The limits of one run.
pub(super) struct Limits<'a> {
    pub timeout: Duration,
    /// A file that the child writes, and the most bytes it may hold.
    pub watch: Option<(&'a Path, u64)>,
}

/// Run `exe` with `args` in `sandbox` and wait for it under `limits`. The
/// arguments go to the program as they are: no shell reads them. The
/// child has no standard input. A child that passes a limit is stopped
/// before this returns.
pub(super) fn run(exe: &Path, args: &[&OsStr], sandbox: &Sandbox, limits: &Limits<'_>) -> Result<Output, RunError> {
    let out_path = sandbox.path().join("stdout.txt");
    let err_path = sandbox.path().join("stderr.txt");
    let mut child = Command::new(exe)
        .args(args)
        .current_dir(sandbox.path())
        .stdin(Stdio::null())
        .stdout(File::create(&out_path)?)
        .stderr(File::create(&err_path)?)
        .spawn()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                stop(&mut child);
                return Err(RunError::Io(e));
            }
        }
        let refusal = if started.elapsed() >= limits.timeout {
            Some(RunError::Timeout)
        } else if len(&out_path) > CAPTURE_MAX || len(&err_path) > CAPTURE_MAX {
            Some(RunError::Flood)
        } else if limits.watch.is_some_and(|(file, max)| len(file) > max) {
            Some(RunError::Oversize)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            stop(&mut child);
            return Err(refusal);
        }
        std::thread::sleep(POLL);
    };
    // The child can pass a limit between the last look and its end.
    if len(&out_path) > CAPTURE_MAX || len(&err_path) > CAPTURE_MAX {
        return Err(RunError::Flood);
    }
    let stdout = read_bounded(&out_path, CAPTURE_MAX)?;
    let stderr = String::from_utf8_lossy(&read_bounded(&err_path, CAPTURE_MAX)?).into_owned();
    Ok(Output { code: status.code(), stdout, stderr })
}

/// Stop the child and collect it. The two results are not needed: a child
/// that ended by itself in the meantime is the same outcome.
fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The size of `path`; 0 when it does not exist.
pub(super) fn len(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

/// The bytes of `path`, never more than `max`.
pub(super) fn read_bounded(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    File::open(path)?.take(max).read_to_end(&mut data)?;
    Ok(data)
}
