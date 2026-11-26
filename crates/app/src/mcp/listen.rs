//! The live transport (#183): a loopback-only TCP listener carrying the
//! same newline-delimited JSON-RPC `camcraft-cli mcp` speaks on stdio,
//! handing each line to the owning (UI) thread over a channel and
//! writing that thread's answer back to the client.
//! The same loop serves the UI link (#3751, [`Inbox::link`]): a local
//! socket whose first line must carry the token of its discovery file.
//! Provenance: our own std::net + std::sync::mpsc code; framing is the
//! MCP stdio transport (modelcontextprotocol.io, 2025-06-18). Clean-room.
//! The thread moves text: one client at a time; dropped clients or closed
//! channels never panic. Dropping [`Inbox`] stops and joins the thread.
//!
//! **Only JSON-RPC speaks** (#474): any non-JSON line closes the
//! connection before reaching the app. Reads are bounded to [`MAX_LINE`]
//! ([`MAX_HELLO_LINE`] before hello, #3873) and writes time out.
//! **An idle client lets go** (#567, #3873): a client idle for
//! [`IDLE_TIMEOUT`] ([`HELLO_TIMEOUT`] before hello) is dropped so the
//! next client can be served.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use crate::clock::Instant;
use std::time::Duration;

use super::link::{self, Registration};
use super::wire::{Listener, Stream};

/// How often a blocked wait looks at the stop flag: the bound on how
/// long dropping an [`Inbox`] can take.
const POLL: Duration = Duration::from_millis(50);

/// A line longer than this is refused and its client dropped - a client
/// that never sends a newline must not grow the app's memory forever.
const MAX_LINE: usize = 16 * 1024 * 1024;

/// A line longer than this before the UI link hello is refused (#3873).
const MAX_HELLO_LINE: usize = 4 * 1024;

/// How long a write to a client may block before the client is dropped -
/// a client that stops reading must not hold the listener.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// The most one read takes off the socket before the idle deadline is
/// looked at again.
const CHUNK: usize = 8 * 1024;

/// How long a client may go without sending a complete line before it
/// is dropped so the next one can be served (#567).
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How long an unauthenticated client may hold the UI link before
/// sending a valid hello line (#3873).
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// How a served client's session ended, for the listener's log line.
enum End {
    /// It hung up, was refused, or the app is stopping.
    Closed,
    /// It sent no complete line for the idle window.
    Idle(Duration),
    /// Its first line was not the UI link's hello with the token (#3751).
    Refused,
}

/// Whether a wire line may be handed to the app: JSON-RPC is always an
/// object or a batch array. An HTTP request line or header is neither.
#[must_use]
pub(crate) fn is_json_rpc_line(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_none_or(|b| *b == b'{' || *b == b'[')
}

/// The waker the listener calls after queueing a request, so an idle
/// GUI loop draws (and drains) a frame. Any thread may call it.
pub type Wake = Box<dyn Fn() + Send + 'static>;

/// One wire line from the client, waiting for the owning thread.
pub struct Request {
    line: String,
    reply: Sender<Option<String>>,
}

impl Request {
    /// An in-process request and the receiver its answer arrives on -
    /// what the listener makes per line, and what a test or an embedder
    /// hands an [`Inbox::channel`] directly.
    #[must_use]
    pub fn pair(line: impl Into<String>) -> (Self, Receiver<Option<String>>) {
        let (reply, rx) = mpsc::channel();
        (Self { line: line.into(), reply }, rx)
    }

    #[must_use]
    pub fn line(&self) -> &str {
        &self.line
    }

    /// Send the answer back (`None` for a notification). A client that
    /// went away meanwhile is logged, not an error for the caller.
    pub fn answer(self, reply: Option<String>) {
        if self.reply.send(reply).is_err() {
            tracing::info!("mcp: the client left before its answer was ready");
        }
    }
}

/// The owning thread's end: requests to answer, and (when listening on
/// a socket) the listener thread it stops and joins on drop.
pub struct Inbox {
    rx: Receiver<Request>,
    addr: Option<SocketAddr>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// The UI link's discovery file (#3751); removed after the thread ends.
    link: Option<Registration>,
}

impl Inbox {
    /// Listen on 127.0.0.1:`port` (0 picks a free port; [`Inbox::addr`]
    /// says which). Loopback only by construction: no other interface
    /// is ever bound.
    ///
    /// # Errors
    /// The bind (port in use, no permission) or the thread spawn failed.
    pub fn listen(port: u16, wake: Wake) -> std::io::Result<Self> {
        Self::listen_with_idle(port, wake, IDLE_TIMEOUT)
    }

    /// [`Inbox::listen`] with its own idle window: a client sending no
    /// complete line for `idle` is dropped (a window shorter than the
    /// poll step is read as the poll step). Tests use a short one.
    ///
    /// # Errors
    /// The bind (port in use, no permission) or the thread spawn failed.
    pub fn listen_with_idle(port: u16, wake: Wake, idle: Duration) -> std::io::Result<Self> {
        let idle = idle.max(POLL);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("camcraft-mcp".into())
            .spawn(move || accept_loop(&Listener::Tcp(listener), &tx, &flag, &wake, idle, None, HELLO_TIMEOUT))?;
        tracing::info!("mcp: listening on {addr} (an idle client is dropped after {}s)", idle.as_secs());
        Ok(Self { rx, addr: Some(addr), stop, thread: Some(thread), link: None })
    }

    /// Open the UI link (#3751) in the link folder `dir`: a local socket
    /// for `camcraft-cli --ui`, found through a discovery file and gated
    /// by its token. The file goes when the inbox drops.
    ///
    /// # Errors
    /// The folder, the bind, the file or the thread spawn failed.
    pub fn link(dir: &Path, wake: Wake) -> std::io::Result<Self> {
        Self::link_with_timeouts(dir, wake, IDLE_TIMEOUT, HELLO_TIMEOUT)
    }

    /// [`Inbox::link`] with custom deadlines (#3873: tests use a short one).
    pub fn link_with_timeouts(dir: &Path, wake: Wake, idle: Duration, hello_timeout: Duration) -> std::io::Result<Self> {
        let (listener, registration, token) = link::open(dir)?;
        listener.set_nonblocking(true)?;
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("camcraft-ui-link".into())
            .spawn(move || accept_loop(&listener, &tx, &flag, &wake, idle, Some(&token), hello_timeout))?;
        tracing::info!("ui link: {} {} (camcraft-cli --ui)", registration.entry().transport, registration.entry().address);
        Ok(Self { rx, addr: None, stop, thread: Some(thread), link: Some(registration) })
    }

    /// The UI link's discovery entry, when this inbox is one.
    #[must_use]
    pub fn link_entry(&self) -> Option<&link::Entry> {
        self.link.as_ref().map(Registration::entry)
    }

    /// Keep the discovery file's project current (`--window <project>`);
    /// a no-op unless this inbox is a UI link and the project changed.
    pub fn note_project(&mut self, project: Option<&Path>) {
        if let Some(registration) = self.link.as_mut() {
            registration.note_project(project);
        }
    }

    /// An inbox with no socket: requests arrive through the returned
    /// sender. The in-process door the UI tests drive.
    #[must_use]
    pub fn channel() -> (Sender<Request>, Self) {
        let (tx, rx) = mpsc::channel();
        (tx, Self { rx, addr: None, stop: Arc::new(AtomicBool::new(false)), thread: None, link: None })
    }

    /// The next waiting request, never blocking.
    #[must_use]
    pub fn next(&self) -> Option<Request> {
        // Empty and disconnected alike: nothing to answer this frame.
        self.rx.try_recv().ok()
    }

    /// The bound address, when listening on a socket.
    #[must_use]
    pub fn addr(&self) -> Option<SocketAddr> {
        self.addr
    }
}

impl Drop for Inbox {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("mcp: the listener thread ended abnormally");
        }
    }
}

fn accept_loop(listener: &Listener, tx: &Sender<Request>, stop: &AtomicBool, wake: &Wake, idle: Duration, token: Option<&str>, hello: Duration) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => match serve_client(stream, tx, stop, wake, idle, token, hello) {
                Ok(End::Closed) => tracing::info!("mcp: client {peer} left"),
                Ok(End::Idle(after)) => tracing::warn!("mcp: client {peer} dropped: idle ({:.1}s)", after.as_secs_f64()),
                Ok(End::Refused) => tracing::warn!("mcp: client {peer} refused: no valid UI link token"),
                Err(e) => tracing::warn!("mcp: client {peer} dropped: {e}"),
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(POLL),
            Err(e) => {
                tracing::warn!("mcp: accept failed: {e}");
                std::thread::sleep(POLL);
            }
        }
    }
}

/// One client until it hangs up, the app stops, or idle/hello expires (#567, #3873).
fn serve_client(
    mut stream: Stream,
    tx: &Sender<Request>,
    stop: &AtomicBool,
    wake: &Wake,
    idle: Duration,
    token: Option<&str>,
    hello: Duration,
) -> std::io::Result<End> {
    let mut gate = token;
    stream.set_nonblocking(false)?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    let mut buf = Vec::new();
    let mut chunk = [0_u8; CHUNK];
    let mut active = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(End::Closed);
        }
        let cap = if gate.is_some() { MAX_HELLO_LINE } else { MAX_LINE };
        let deadline = if gate.is_some() { hello } else { idle };
        let quiet = active.elapsed();
        let Some(left) = deadline.checked_sub(quiet).filter(|d| !d.is_zero()) else {
            if gate.is_some() {
                let _ = writeln!(writer, "{}", link::HELLO_REFUSED).and_then(|()| writer.flush());
                return Ok(End::Refused);
            }
            let note = format!(
                r#"{{"jsonrpc":"2.0","method":"notifications/message","params":{{"level":"warning","logger":"camcraft","data":"idle for {}s with no complete message - the connection is closed"}}}}"#,
                quiet.as_secs()
            );
            // Best effort: the write times out, and the client goes either way.
            let _ = writeln!(writer, "{note}").and_then(|()| writer.flush());
            return Ok(End::Idle(quiet));
        };
        // Never wait past the idle deadline (nor a poll step, for the
        // stop flag). A zero timeout is refused by the OS API; `left`
        // is non-zero, and a sub-millisecond rest rounds up to 1 ms.
        stream.set_read_timeout(Some(left.min(POLL).max(Duration::from_millis(1))))?;
        // Bounded as it reads: never more than one byte past the cap.
        let room = (cap + 1).saturating_sub(buf.len()).clamp(1, CHUNK);
        let got = match chunk.get_mut(..room).map(|c| stream.read(c)) {
            Some(Ok(n)) => n,
            Some(Err(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Some(Err(e)) => return Err(e),
            None => 0,
        };
        if got == 0 {
            // End of input: a last unterminated line still counts, as
            // it does on stdio.
            if !buf.is_empty() && gate.is_none() {
                let line = std::mem::take(&mut buf);
                forward(&line, tx, &mut writer, stop, wake)?;
            }
            return Ok(End::Closed);
        }
        buf.extend_from_slice(chunk.get(..got).unwrap_or_default());
        // Every complete line in what arrived, in order.
        while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
            let rest = buf.split_off(nl.saturating_add(1));
            let line = std::mem::replace(&mut buf, rest);
            if let Some(want) = gate.take() {
                let Some(proof) = link::hello_matches(&line, want) else {
                    let _ = writeln!(writer, "{}", link::HELLO_REFUSED).and_then(|()| writer.flush());
                    return Ok(End::Refused);
                };
                writeln!(writer, "{}", link::hello_ok(&proof))?;
                writer.flush()?;
                active = Instant::now();
                continue;
            }
            if !forward(&line, tx, &mut writer, stop, wake)? {
                return Ok(End::Closed);
            }
            active = Instant::now();
        }
        if buf.len() > cap {
            if gate.is_some() {
                let _ = writeln!(writer, "{}", link::HELLO_REFUSED).and_then(|()| writer.flush());
                return Ok(End::Refused);
            }
            let refusal = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"message too long - the connection is closed"}}"#;
            writeln!(writer, "{refusal}")?;
            writer.flush()?;
            return Ok(End::Closed);
        }
    }
}

/// Hand one line to the owning thread and write its answer. `Ok(false)`
/// when the session cannot go on (the app is stopping or gone, or the
/// client is not speaking JSON-RPC).
fn forward(bytes: &[u8], tx: &Sender<Request>, writer: &mut Stream, stop: &AtomicBool, wake: &Wake) -> std::io::Result<bool> {
    if !is_json_rpc_line(bytes) {
        tracing::warn!("mcp: a client sent a line that is not JSON-RPC (an HTTP request?) - connection closed");
        let refusal = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"not a JSON-RPC message - the connection is closed"}}"#;
        let _ = writeln!(writer, "{refusal}").and_then(|()| writer.flush());
        return Ok(false);
    }
    let line = String::from_utf8_lossy(bytes).into_owned();
    if line.trim().is_empty() {
        return Ok(true);
    }
    let (request, answer) = Request::pair(line);
    if tx.send(request).is_err() {
        tracing::info!("mcp: the app closed its end; ending the session");
        return Ok(false);
    }
    wake();
    loop {
        match answer.recv_timeout(POLL) {
            Ok(Some(reply)) => {
                writeln!(writer, "{reply}")?;
                writer.flush()?;
                return Ok(true);
            }
            Ok(None) => return Ok(true),
            Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Relaxed) => return Ok(false),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                tracing::warn!("mcp: a request was dropped unanswered");
                return Ok(true);
            }
        }
    }
}
