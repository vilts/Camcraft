//! The byte streams the live listener serves (#183, #3751): a loopback
//! TCP socket (`--mcp-port`, and the UI link on a platform where std has
//! no local socket) or, on Unix, a Unix domain socket in the user's own
//! folder. Both carry the same newline-delimited JSON-RPC; this module
//! only hides which one it is.
//!
//! Provenance: our own wrapper over std::net and std::os::unix::net (the
//! Rust standard library docs). Clean-room, public sources only.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

/// A listening socket: what [`super::Inbox`] accepts clients on.
pub enum Listener {
    Tcp(TcpListener),
    #[cfg(unix)]
    Unix(UnixListener),
}

impl Listener {
    /// Accept one client, never blocking (the listener is non-blocking):
    /// the stream and a label for the log lines.
    ///
    /// # Errors
    /// `WouldBlock` when no client waits, else the OS error.
    pub fn accept(&self) -> std::io::Result<(Stream, String)> {
        match self {
            Listener::Tcp(l) => l.accept().map(|(s, peer)| (Stream::Tcp(s), peer.to_string())),
            #[cfg(unix)]
            Listener::Unix(l) => l.accept().map(|(s, _)| (Stream::Unix(s), "local socket".to_owned())),
        }
    }

    /// # Errors
    /// The OS refused the mode change.
    pub fn set_nonblocking(&self, on: bool) -> std::io::Result<()> {
        match self {
            Listener::Tcp(l) => l.set_nonblocking(on),
            #[cfg(unix)]
            Listener::Unix(l) => l.set_nonblocking(on),
        }
    }
}

/// One connected client (or, for the CLI, the connection to the app).
pub enum Stream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl Stream {
    /// # Errors
    /// The OS refused the mode change.
    pub fn set_nonblocking(&self, on: bool) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => s.set_nonblocking(on),
            #[cfg(unix)]
            Stream::Unix(s) => s.set_nonblocking(on),
        }
    }

    /// # Errors
    /// A zero duration, or the OS refused it.
    pub fn set_read_timeout(&self, t: Option<Duration>) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => s.set_read_timeout(t),
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(t),
        }
    }

    /// # Errors
    /// A zero duration, or the OS refused it.
    pub fn set_write_timeout(&self, t: Option<Duration>) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => s.set_write_timeout(t),
            #[cfg(unix)]
            Stream::Unix(s) => s.set_write_timeout(t),
        }
    }

    /// A second handle on the same connection (one reads, one writes).
    ///
    /// # Errors
    /// The OS could not duplicate the handle.
    pub fn try_clone(&self) -> std::io::Result<Self> {
        match self {
            Stream::Tcp(s) => s.try_clone().map(Stream::Tcp),
            #[cfg(unix)]
            Stream::Unix(s) => s.try_clone().map(Stream::Unix),
        }
    }

    /// Close the sending half, so the other end reads the end of input.
    ///
    /// # Errors
    /// The OS refused it (the connection is gone already).
    pub fn shutdown_write(&self) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => s.shutdown(std::net::Shutdown::Write),
            #[cfg(unix)]
            Stream::Unix(s) => s.shutdown(std::net::Shutdown::Write),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
        }
    }
}
