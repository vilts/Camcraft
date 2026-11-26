//! The CLI's end of the UI link (#3751): find a running window from the
//! discovery files, open a session with its token, and send it MCP
//! `tools/call` lines - the same tool surface `camcraft-cli mcp` serves,
//! answered on the window's UI thread through its History, so Ctrl+Z in
//! the window undoes a CLI step. Never falls back to a headless
//! document: no window is an error by name.
//!
//! Provenance: our own std code over our own link (link.rs); the framing
//! is the MCP stdio transport (modelcontextprotocol.io, revision
//! 2025-06-18). Clean-room, public sources only.

use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use super::link::{self, Entry};
use super::link_owner::{self, Owner};
use super::wire::Stream;

/// How long a connect and the hello answer may take before a window is
/// read as not answering (a stale file, a hung window).
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// Which window to talk to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Select {
    /// The one running window; an error when there are several.
    Only,
    /// The window of this process id.
    Pid(u32),
    /// The window that has this project open.
    Project(PathBuf),
}

impl Select {
    /// `--window <pid|project path>`: all digits is a process id.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.parse::<u32>() {
            Ok(pid) => Select::Pid(pid),
            Err(_) => Select::Project(PathBuf::from(text)),
        }
    }

    fn admits(&self, entry: &Entry) -> bool {
        match self {
            Select::Only => true,
            Select::Pid(pid) => entry.pid == *pid,
            Select::Project(want) => entry.project.as_deref().is_some_and(|p| same_path(p, want)),
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    a == b || canon(a) == canon(b)
}

/// Why a `--ui` call did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// No window answered: nothing ran. Exit 3.
    NoWindow(String),
    /// Several windows answered and none was chosen. Exit 2.
    Choose(String),
    /// The window refused the session or the connection failed. Exit 1.
    Failed(String),
    /// The window is busy with another call. Exit 4.
    Busy(String),
}

impl LinkError {
    /// The CLI's exit code for this error.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            LinkError::NoWindow(_) => 3,
            LinkError::Choose(_) => 2,
            LinkError::Failed(_) => 1,
            LinkError::Busy(_) => 4,
        }
    }
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::NoWindow(m)
            | LinkError::Choose(m)
            | LinkError::Failed(m)
            | LinkError::Busy(m) => f.write_str(m),
        }
    }
}

fn describe(entry: &Entry) -> String {
    let project = entry
        .project
        .as_ref()
        .map_or_else(|| "an unsaved document".to_owned(), |p| p.display().to_string());
    format!("pid {} ({project})", entry.pid)
}

/// Connect to one window and pass its token check.
fn open(entry: &Entry) -> std::io::Result<Stream> {
    let stream = match entry.transport.as_str() {
        "tcp" => {
            let addr: std::net::SocketAddr = entry
                .address
                .parse()
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "not a socket address"))?;
            // Never send a token off this machine, whatever the file says.
            if !addr.ip().is_loopback() {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a loopback address"));
            }
            Stream::Tcp(std::net::TcpStream::connect_timeout(&addr, HELLO_TIMEOUT)?)
        }
        #[cfg(unix)]
        "unix" => Stream::Unix(std::os::unix::net::UnixStream::connect(&entry.address)?),
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("transport '{other}' is not served on this platform"),
            ));
        }
    };
    stream.set_read_timeout(Some(HELLO_TIMEOUT))?;
    stream.set_write_timeout(Some(HELLO_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    let nonce = link::new_nonce();
    writeln!(writer, "{}", link::hello_line(&entry.token, &nonce))?;
    writer.flush()?;
    let mut answer = String::new();
    BufReader::new(stream.try_clone()?).take(4096).read_line(&mut answer)?;
    let Ok(resp) = serde_json::from_str::<Value>(answer.trim()) else {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "unreadable hello answer"));
    };
    if resp.get("ok") != Some(&json!(true)) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the window refused the link token"));
    }
    let expected_proof = link::token_proof(&entry.token, &nonce, "server");
    let Some(proof) = resp.get("proof").and_then(Value::as_str) else {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the window did not prove the link token"));
    };
    if !link::constant_time_eq(proof.as_bytes(), expected_proof.as_bytes()) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the window failed to prove the link token"));
    }
    // A tool may run for minutes (generate): no read deadline from here.
    stream.set_read_timeout(None)?;
    stream.set_write_timeout(None)?;
    Ok(stream)
}

/// Find the selected window in `dir` and open a session with it.
///
/// # Errors
/// No window answered, several answered without a choice, or the chosen
/// window refused the token.
pub fn connect(dir: &Path, select: &Select) -> Result<(Entry, Stream), LinkError> {
    let all = link::entries(dir);
    if all.is_empty() {
        return Err(LinkError::NoWindow(format!(
            "no running CamCraft window: no link file in {} - start camcraft first (nothing ran)",
            dir.display()
        )));
    }
    let candidates: Vec<&Entry> = all.iter().filter(|e| select.admits(e)).collect();
    if candidates.is_empty() {
        let running: Vec<String> = all.iter().map(describe).collect();
        let wanted = match select {
            Select::Only => "a window".to_owned(),
            Select::Pid(pid) => format!("process id {pid}"),
            Select::Project(p) => format!("project {}", p.display()),
        };
        return Err(LinkError::NoWindow(format!(
            "no CamCraft window with {wanted}; link files name: {} (nothing ran)",
            running.join(", ")
        )));
    }
    let mut refused = None;
    let mut busy = None;
    let mut unproved = Vec::new();
    let mut open_ones = Vec::new();
    for entry in candidates {
        // #3995: only a file whose window is proved alive is dialed.
        if let Some(why) = not_alive(dir, entry) {
            unproved.push(why);
            continue;
        }
        match open(entry) {
            // Proved again after the hello: the window held its lock for the
            // whole exchange, so the peer that answered is its own socket.
            Ok(stream) => match not_alive(dir, entry) {
                None => open_ones.push((entry.clone(), stream)),
                Some(why) => unproved.push(why),
            },
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => refused = Some(describe(entry)),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => {
                tracing::info!("ui link: {} is busy: {e}", describe(entry));
                busy = Some(describe(entry));
            }
            // A stale file of a closed window: its socket does not answer.
            Err(e) => tracing::info!("ui link: {} does not answer: {e}", describe(entry)),
        }
    }
    match open_ones.len() {
        1 => open_ones.pop().ok_or_else(|| LinkError::Failed("ui link: lost the open session".into())),
        0 => Err(if let Some(who) = refused {
            LinkError::Failed(format!(
                "the CamCraft window {who} refused the link token in its link file (nothing ran)"
            ))
        } else if let Some(who) = busy {
            LinkError::Busy(format!(
                "the CamCraft window {who} is busy with another call (nothing ran)"
            ))
        } else {
            LinkError::NoWindow(format!(
                "no running CamCraft window answered: the link files in {} are stale or unproved{} (nothing ran)",
                dir.display(),
                if unproved.is_empty() { String::new() } else { format!(": {}", unproved.join("; ")) }
            ))
        }),
        n => {
            let names: Vec<String> = open_ones.iter().map(|(e, _)| describe(e)).collect();
            Err(LinkError::Choose(format!(
                "{n} CamCraft windows are running: {}; choose one with --window <pid|project path> (nothing ran)",
                names.join(", ")
            )))
        }
    }
}

/// `None` when the lock of `entry`'s window proves it alive; else why the
/// entry is not used, logged. A stale entry's files are removed.
fn not_alive(dir: &Path, entry: &Entry) -> Option<String> {
    match link_owner::owner(dir, entry.pid) {
        Owner::Alive => None,
        Owner::Gone => {
            let why = format!("{} is stale: its window has ended; its link file is removed", describe(entry));
            tracing::warn!("ui link: {why}");
            link_owner::remove_stale(dir, entry.pid);
            Some(why)
        }
        Owner::Unproved(reason) => {
            let why = format!("{} is not used: nothing proves that its window runs ({reason})", describe(entry));
            tracing::warn!("ui link: {why}");
            Some(why)
        }
    }
}

/// Maximum byte length for one answer line from a window (16 MiB, #3874).
pub const MAX_ANSWER_LINE: usize = 16 * 1024 * 1024;

/// Error from [`read_answer_line`].
#[derive(Debug, PartialEq, Eq)]
pub enum ReadLineError {
    /// The stream reached end-of-file before any bytes were read.
    Eof,
    /// The line exceeded the maximum allowed byte length.
    TooLong { limit: usize, bytes_read: usize },
    /// An I/O error occurred while reading from the stream.
    Io(String),
}

impl fmt::Display for ReadLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eof => write!(f, "end of input"),
            Self::TooLong { limit, bytes_read } => {
                write!(f, "line exceeded limit of {limit} bytes (read {bytes_read} bytes)")
            }
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

/// Read one line up to `limit` bytes.
///
/// # Errors
/// Returns [`ReadLineError::TooLong`] when the line exceeds `limit`.
pub fn read_answer_line(reader: &mut impl BufRead, limit: usize) -> Result<String, ReadLineError> {
    let mut buf = Vec::new();
    let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    match reader.by_ref().take(cap).read_until(b'\n', &mut buf) {
        Ok(0) => Err(ReadLineError::Eof),
        Ok(n) if n > limit => Err(ReadLineError::TooLong { limit, bytes_read: n }),
        Ok(_) => String::from_utf8(buf).map_err(|e| ReadLineError::Io(format!("the line is not UTF-8: {e}"))),
        Err(e) => Err(ReadLineError::Io(e.to_string())),
    }
}

/// Send one `tools/call` to the selected window and return its MCP
/// result (`content`, `structuredContent`, `isError`).
///
/// # Errors
/// See [`connect`]; or the session broke before the answer came.
pub fn call(dir: &Path, select: &Select, tool: &str, args: &Value) -> Result<Value, LinkError> {
    let (entry, stream) = connect(dir, select)?;
    let lost = |e: std::io::Error| LinkError::Failed(format!("the link to {} broke: {e}", describe(&entry)));
    let mut writer = stream.try_clone().map_err(lost)?;
    let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": args } });
    writeln!(writer, "{line}").and_then(|()| writer.flush()).map_err(lost)?;
    let mut reader = BufReader::new(stream);
    loop {
        let text = match read_answer_line(&mut reader, MAX_ANSWER_LINE) {
            Ok(t) => t,
            Err(ReadLineError::Eof) => {
                return Err(LinkError::Failed(format!(
                    "the window {} closed the link before it answered",
                    describe(&entry)
                )));
            }
            Err(ReadLineError::TooLong { limit, .. }) => {
                return Err(LinkError::Failed(format!(
                    "the window {} sent an answer line exceeding the limit of {limit} bytes",
                    describe(&entry)
                )));
            }
            Err(ReadLineError::Io(e)) => {
                return Err(LinkError::Failed(format!("the link to {} broke: {e}", describe(&entry))));
            }
        };
        let Ok(msg) = serde_json::from_str::<Value>(text.trim()) else { continue };
        // A notification (an idle note) has no id; only our answer counts.
        if msg.get("id") != Some(&json!(1)) {
            continue;
        }
        if let Some(result) = msg.get("result") {
            return Ok(result.clone());
        }
        let why = msg["error"]["message"].as_str().unwrap_or("an unreadable answer");
        return Err(LinkError::Failed(format!("the window refused the call: {why}")));
    }
}

/// `--ui mcp`: a stdio bridge to the selected window's session, for an
/// MCP client that launches stdio servers.
///
/// # Errors
/// See [`connect`]; or a stream failed.
pub fn bridge(dir: &Path, select: &Select, input: impl Read + Send + 'static, mut output: impl Write) -> Result<(), LinkError> {
    let (entry, mut from_app) = connect(dir, select)?;
    let lost = |e: std::io::Error| LinkError::Failed(format!("the link to {} broke: {e}", describe(&entry)));
    let mut to_app = from_app.try_clone().map_err(lost)?;
    // Detached on purpose: a blocked stdin read cannot be interrupted.
    std::thread::Builder::new()
        .name("camcraft-ui-link-bridge".into())
        .spawn(move || {
            let mut input = input;
            if let Err(e) = std::io::copy(&mut input, &mut to_app) {
                tracing::warn!("ui link bridge: sending failed: {e}");
            }
            let _ = to_app.shutdown_write();
        })
        .map_err(lost)?;
    std::io::copy(&mut from_app, &mut output).map_err(lost)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_answer_line_accepts_valid_line() {
        let mut reader = Cursor::new(b"{\"id\":1,\"result\":{\"ok\":true}}\n");
        let line = read_answer_line(&mut reader, 100).expect("valid line");
        assert_eq!(line, "{\"id\":1,\"result\":{\"ok\":true}}\n");
    }

    #[test]
    fn read_answer_line_rejects_oversized_line() {
        let data = b"0123456789extra_bytes\n";
        let mut reader = Cursor::new(data);
        let err = read_answer_line(&mut reader, 10).expect_err("oversized line");
        assert_eq!(err, ReadLineError::TooLong { limit: 10, bytes_read: 11 });
    }

    #[test]
    fn read_answer_line_rejects_oversized_line_without_newline() {
        let data = b"0123456789extra_bytes_no_nl";
        let mut reader = Cursor::new(data);
        let err = read_answer_line(&mut reader, 10).expect_err("oversized line without newline");
        assert_eq!(err, ReadLineError::TooLong { limit: 10, bytes_read: 11 });
    }
}
