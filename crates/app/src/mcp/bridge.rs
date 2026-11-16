//! `camcraft-cli mcp --connect <addr>` (#183): a stdio <-> TCP bridge, so
//! an MCP client that only launches stdio servers can drive a running
//! app's live session. Bytes are copied both ways untouched; the app
//! end speaks the protocol.
//!
//! Provenance: our own std::net code; the MCP stdio transport
//! (modelcontextprotocol.io, revision 2025-06-18) on the client side.
//! Clean-room, public sources only.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};

/// Copy `input` to the app at `addr` and its answers to `output` until
/// the app closes the connection. End of `input` half-closes the socket,
/// so the answers still in flight arrive before the bridge ends.
///
/// # Errors
/// The address does not resolve or connect, or a stream fails.
pub fn bridge(addr: &str, input: impl Read + Send + 'static, mut output: impl Write) -> std::io::Result<()> {
    let target = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("'{addr}' names no address")))?;
    let mut from_app = TcpStream::connect(target)?;
    let mut to_app = from_app.try_clone()?;
    // Detached on purpose: a blocked stdin read cannot be interrupted, and
    // the process ends when the app's side closes anyway.
    let _pump = std::thread::Builder::new().name("camcraft-mcp-bridge".into()).spawn(move || {
        let mut input = input;
        if let Err(e) = std::io::copy(&mut input, &mut to_app) {
            tracing::warn!("mcp bridge: sending failed: {e}");
        }
        let _ = to_app.shutdown(Shutdown::Write);
    })?;
    let mut chunk = [0u8; 8192];
    loop {
        let n = from_app.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        output.write_all(&chunk[..n])?;
        output.flush()?;
    }
}
