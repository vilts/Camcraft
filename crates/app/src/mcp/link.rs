//! The UI link (#3751): each running CamCraft window opens a local
//! socket at start and writes a discovery file, so `camcraft-cli --ui`
//! can find it and send it MCP tool calls that run on the window's own
//! History. This module is the window's end: the folder, the token, the
//! discovery file and the hello line that opens a session.
//!
//! - **Local only.** On Unix the socket is a Unix domain socket in the
//!   user's own folder (mode 0700, the socket 0600). Elsewhere (Windows:
//!   std has no named pipe or local socket) it is TCP bound to 127.0.0.1
//!   with an OS-chosen port; no other interface is ever bound.
//! - **Same user only.** A client must open with the random token from
//!   the discovery file, which only the user can read; any other first
//!   line closes the connection before a byte of it reaches the app.
//! - **One file per window.** `<pid>.json` in the link folder; a window
//!   removes its file when it closes. While it runs it holds the OS lock
//!   of `<pid>.lock` (link_owner.rs, #3995). A file left by a crash is
//!   stale: its lock is free, so the client removes it and never dials
//!   its address, which a false server can hold.
//!
//! Provenance: our own design (docs/decisions/3751-cli-ui-link.md) over
//! std; the per-user folders are the platforms' published locations
//! (Windows `%APPDATA%`, macOS `~/Library/Application Support`, the XDG
//! Base Directory spec with `XDG_RUNTIME_DIR` for sockets). Clean-room,
//! public sources only.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::wire::Listener;

/// The override of the link folder: an absolute path (tests use one).
pub const DIR_ENV: &str = "CAMCRAFT_UI_LINK_DIR";
/// `CAMCRAFT_UI_LINK=off` starts a window without its link.
pub const OFF_ENV: &str = "CAMCRAFT_UI_LINK";
/// The discovery file format this build writes and reads.
pub const FORMAT: u32 = 1;
/// The window's answer to a good hello line.
pub const HELLO_OK: &str = r#"{"camcraft_link":1,"ok":true}"#;
/// The window's answer to any other first line; then it hangs up.
pub const HELLO_REFUSED: &str = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32001,"message":"camcraft UI link: wrong or missing token - the connection is closed"}}"#;

/// One window's discovery file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub format: u32,
    /// The window's process id.
    pub pid: u32,
    /// `"unix"` (a socket path) or `"tcp"` (a 127.0.0.1 address).
    pub transport: String,
    pub address: String,
    pub token: String,
    /// The project the window has open, `None` for an unsaved document.
    pub project: Option<PathBuf>,
    pub app_version: String,
}

/// The link folder from an environment lookup, for the named platform
/// (`std::env::consts::OS`). Pure, so the order is testable. `None` = no
/// per-user folder to put one in.
#[must_use]
pub fn dir_for(os: &str, get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let abs = |k: &str| get(k).map(PathBuf::from).filter(|p| p.is_absolute());
    if let Some(dir) = abs(DIR_ENV) {
        return Some(dir);
    }
    let base = match os {
        "windows" => abs("APPDATA"),
        "macos" => abs("HOME").map(|h| h.join("Library").join("Application Support")),
        // A runtime folder is the user's own (0700) and short, which a
        // socket path needs (about 100 bytes at most).
        _ => abs("XDG_RUNTIME_DIR")
            .or_else(|| abs("XDG_DATA_HOME"))
            .or_else(|| abs("HOME").map(|h| h.join(".local").join("share"))),
    }?;
    Some(base.join("camcraft").join("ui-link"))
}

/// The link folder for this process.
#[must_use]
pub fn dir() -> Option<PathBuf> {
    dir_for(std::env::consts::OS, |k| std::env::var_os(k))
}

/// Whether a window starts its link: on, unless `CAMCRAFT_UI_LINK=off`.
#[must_use]
pub fn enabled(env: Option<&str>) -> bool {
    !env.is_some_and(|v| v.trim().eq_ignore_ascii_case("off"))
}

/// A fresh token: 256 bits of SipHash output under keys that std seeds
/// from the OS's random source (`RandomState`), mixed with the clock and
/// the process id. No new dependency (decision 3751).
fn new_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (0..4_u64)
        .map(|i| {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(nanos);
            h.write_u32(std::process::id());
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// The window's answer to a good hello line with its token proof (#3874).
#[must_use]
pub fn hello_ok(proof: &str) -> String {
    json!({ "camcraft_link": FORMAT, "ok": true, "proof": proof }).to_string()
}

/// Constant-time comparison of two byte slices.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Keyed proof that proves knowledge of the token without revealing it (#3874).
#[must_use]
pub fn token_proof(token: &str, nonce: &str, role: &str) -> String {
    let k0 = token.get(..16).and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
    let k1 = token.get(16..32).and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
    let msg = format!("{role}:{nonce}");
    format!("{:016x}", siphash24(k0, k1, msg.as_bytes()))
}

/// A random 64-bit nonce for the hello handshake.
#[must_use]
pub fn new_nonce() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{:016x}", nanos ^ (u128::from(std::process::id()) << 32))
}

/// Pure deterministic SipHash-2-4 for token proof (#3874).
fn siphash24(k0: u64, k1: u64, msg: &[u8]) -> u64 {
    let mut v0 = k0 ^ 0x736f_6d65_7073_6575;
    let mut v1 = k1 ^ 0x646f_7261_6e64_6f6d;
    let mut v2 = k0 ^ 0x6c79_6765_6e65_7261;
    let mut v3 = k1 ^ 0x7465_6462_7974_6573;
    let mut b = (msg.len() as u64) << 56;
    let (chunks, rem) = msg.as_chunks::<8>();
    for chunk in chunks {
        let m = u64::from_le_bytes(*chunk);
        v3 ^= m;
        for _ in 0..2 {
            v0 = v0.wrapping_add(v1); v1 = v1.rotate_left(13); v1 ^= v0; v0 = v0.rotate_left(32);
            v2 = v2.wrapping_add(v3); v3 = v3.rotate_left(16); v3 ^= v2;
            v0 = v0.wrapping_add(v3); v3 = v3.rotate_left(21); v3 ^= v0;
            v2 = v2.wrapping_add(v1); v1 = v1.rotate_left(17); v1 ^= v2; v2 = v2.rotate_left(32);
        }
        v0 ^= m;
    }
    for (i, &byte) in rem.iter().enumerate() {
        b |= (byte as u64) << (8 * i);
    }
    v3 ^= b;
    for _ in 0..2 {
        v0 = v0.wrapping_add(v1); v1 = v1.rotate_left(13); v1 ^= v0; v0 = v0.rotate_left(32);
        v2 = v2.wrapping_add(v3); v3 = v3.rotate_left(16); v3 ^= v2;
        v0 = v0.wrapping_add(v3); v3 = v3.rotate_left(21); v3 ^= v0;
        v2 = v2.wrapping_add(v1); v1 = v1.rotate_left(17); v1 ^= v2; v2 = v2.rotate_left(32);
    }
    v0 ^= b;
    v2 ^= 0xff;
    for _ in 0..4 {
        v0 = v0.wrapping_add(v1); v1 = v1.rotate_left(13); v1 ^= v0; v0 = v0.rotate_left(32);
        v2 = v2.wrapping_add(v3); v3 = v3.rotate_left(16); v3 ^= v2;
        v0 = v0.wrapping_add(v3); v3 = v3.rotate_left(21); v3 ^= v0;
        v2 = v2.wrapping_add(v1); v1 = v1.rotate_left(17); v1 ^= v2; v2 = v2.rotate_left(32);
    }
    v0 ^ v1 ^ v2 ^ v3
}

/// The first line a client sends with mutual token proof (#3874).
#[must_use]
pub fn hello_line(token: &str, nonce: &str) -> String {
    json!({
        "camcraft_link": FORMAT,
        "token_proof": token_proof(token, nonce, "client"),
        "nonce": nonce
    }).to_string()
}

/// Whether a first line is the hello with this token. Returns the server proof if valid.
#[must_use]
pub fn hello_matches(line: &[u8], token: &str) -> Option<String> {
    let Ok(v) = serde_json::from_slice::<Value>(line) else { return None };
    if v.get("camcraft_link").and_then(Value::as_u64) != Some(u64::from(FORMAT)) {
        return None;
    }
    let nonce = v.get("nonce").and_then(Value::as_str).unwrap_or("");
    if let Some(proof) = v.get("token_proof").and_then(Value::as_str)
        && constant_time_eq(proof.as_bytes(), token_proof(token, nonce, "client").as_bytes())
    {
        return Some(token_proof(token, nonce, "server"));
    } else if let Some(got) = v.get("token").and_then(Value::as_str)
        && constant_time_eq(got.as_bytes(), token.as_bytes())
    {
        return Some(token_proof(token, nonce, "server"));
    }
    None
}

/// A window's registration: its discovery file (and socket file), removed
/// when it drops.
#[derive(Debug)]
pub struct Registration {
    file: PathBuf,
    socket: Option<PathBuf>,
    entry: Entry,
    lock_file: PathBuf,
    /// Held for the life of the window: the proof that it runs (#3995).
    /// Released by `Drop::drop` after it removed the files.
    lock: std::fs::File,
}

impl Registration {
    #[must_use]
    pub fn entry(&self) -> &Entry {
        &self.entry
    }

    /// Record the window's open project, so `--window <project>` finds it.
    /// Writes only when it changed; a failed write is logged.
    pub fn note_project(&mut self, project: Option<&Path>) {
        if self.entry.project.as_deref() == project {
            return;
        }
        self.entry.project = project.map(Path::to_path_buf);
        if let Err(e) = write_entry(&self.file, &self.entry) {
            tracing::warn!("ui link: cannot update {}: {e}", self.file.display());
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let files = std::iter::once(&self.file).chain(self.socket.as_ref());
        for path in files.chain(std::iter::once(&self.lock_file)) {
            if let Err(e) = std::fs::remove_file(path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!("ui link: cannot remove {}: {e}", path.display());
            }
        }
        // Release the lock now, not at the close of the file (#4041): on
        // Unix a child that another thread is starting holds a copy of the
        // descriptor until its exec, and the close alone leaves the lock
        // held while that copy lives.
        if let Err(e) = self.lock.unlock() {
            tracing::warn!("ui link: cannot release {}: {e}", self.lock_file.display());
        }
    }
}

/// Bind this window's socket in `dir` and write its discovery file.
///
/// # Errors
/// The folder cannot be made private, the bind failed, or the file
/// could not be written (then nothing is left behind).
pub(crate) fn open(dir: &Path) -> std::io::Result<(Listener, Registration, String)> {
    prepare_dir(dir)?;
    let pid = std::process::id();
    let token = new_token();
    // The lock comes first, so no link file of this window is ever unproved.
    let lock = super::link_owner::hold(dir, pid)?;
    let lock_file = super::link_owner::lock_path(dir, pid);
    let (listener, transport, address, socket) = match bind(dir, pid) {
        Ok(bound) => bound,
        Err(e) => {
            let _ = lock.unlock();
            drop(lock);
            let _ = std::fs::remove_file(&lock_file);
            return Err(e);
        }
    };
    let entry = Entry {
        format: FORMAT,
        pid,
        transport: transport.to_owned(),
        address,
        token: token.clone(),
        project: None,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    // Built before the write, so a failed write still removes the socket.
    let file = dir.join(format!("{pid}.json"));
    let registration = Registration { file, socket, entry, lock_file, lock };
    write_entry(&registration.file, &registration.entry)?;
    Ok((listener, registration, token))
}

fn prepare_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(unix)]
fn bind(dir: &Path, pid: u32) -> std::io::Result<(Listener, &'static str, String, Option<PathBuf>)> {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(format!("{pid}.sock"));
    let address = path
        .to_str()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "the link folder path is not UTF-8"))?
        .to_owned();
    // A file of a crashed process with the same pid: nobody serves it.
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok((Listener::Unix(listener), "unix", address, Some(path)))
}

#[cfg(not(unix))]
fn bind(_dir: &Path, _pid: u32) -> std::io::Result<(Listener, &'static str, String, Option<PathBuf>)> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?.to_string();
    Ok((Listener::Tcp(listener), "tcp", address, None))
}

/// Write the file all or nothing (a temp file, then a rename), readable
/// by the user only on Unix.
fn write_entry(file: &Path, entry: &Entry) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(entry).map_err(std::io::Error::other)?;
    let tmp = file.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&tmp)?;
    out.write_all(text.as_bytes())?;
    out.sync_all()?;
    drop(out);
    std::fs::rename(&tmp, file)
}

/// Every readable discovery file in `dir`, by process id. A file that is
/// not one (a temp file, another format) is skipped and logged.
#[must_use]
pub fn entries(dir: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<Entry> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| {
            let parsed = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| serde_json::from_str::<Entry>(&t).ok())
                .filter(|e| e.format == FORMAT);
            if parsed.is_none() {
                tracing::info!("ui link: {} is not a link file of this version; skipped", p.display());
            }
            parsed
        })
        .collect();
    found.sort_by_key(|e| e.pid);
    found
}
