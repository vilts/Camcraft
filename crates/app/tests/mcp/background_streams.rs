//! MCP stream backpressure and output-error regressions from PR #3860.
//! Provenance: our own stdio protocol tests.
//! Gate map: covers app::mcp (background, serve).

use camcraft_app::mcp;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

const PING: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n";

/// Expose exactly one line per buffer and report every consumed line.
struct CountLines {
    data: Cursor<Vec<u8>>,
    consumed: Sender<()>,
}

impl Read for CountLines {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let bytes = self.fill_buf()?;
        let n = bytes.len().min(out.len());
        out[..n].copy_from_slice(&bytes[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl BufRead for CountLines {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let bytes = self.data.fill_buf()?;
        Ok(&bytes[..bytes
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |i| i + 1)])
    }

    fn consume(&mut self, n: usize) {
        let line_ended = self
            .data
            .fill_buf()
            .expect("memory input")
            .get(..n)
            .is_some_and(|b| b.ends_with(b"\n"));
        self.data.consume(n);
        if line_ended {
            let _ = self.consumed.send(());
        }
    }
}

/// The first response blocks until the client releases it, then fails.
struct BlockedOutput {
    entered: Sender<()>,
    release: Receiver<()>,
}

impl Write for BlockedOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        self.entered.send(()).expect("client waiting for response");
        self.release.recv().expect("client releases output");
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "client closed output",
        ))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn blocked_output_bounds_the_input_queue() {
    let (consumed_tx, consumed_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let input = CountLines {
        data: Cursor::new(PING.repeat(1000)),
        consumed: consumed_tx,
    };
    let thread = std::thread::spawn(move || {
        mcp::serve(
            input,
            BlockedOutput {
                entered: entered_tx,
                release: release_rx,
            },
        )
    });
    entered_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("first response starts");
    // One request is being answered, one is queued, and one line waits to send.
    for _ in 0..3 {
        consumed_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("reader fills its finite budget");
    }
    let extra = consumed_rx.recv_timeout(Duration::from_millis(200));
    release_tx.send(()).expect("release output");
    let result = thread.join().expect("session thread");
    assert_eq!(
        result.expect_err("output fails").kind(),
        io::ErrorKind::BrokenPipe
    );
    assert!(
        matches!(extra, Err(mpsc::RecvTimeoutError::Timeout)),
        "reader consumed more than three lines while output was blocked: {extra:?}"
    );
}

/// Keep input open after its first request until the test closes the sender.
struct OpenInput {
    chunks: Receiver<Vec<u8>>,
    current: Cursor<Vec<u8>>,
}

impl Read for OpenInput {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.current.position() == self.current.get_ref().len() as u64 {
            self.current = match self.chunks.recv() {
                Ok(chunk) => Cursor::new(chunk),
                Err(_) => return Ok(0),
            };
        }
        self.current.read(out)
    }
}

#[test]
fn an_output_error_returns_while_input_remains_open() {
    let (input_tx, input_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let input = BufReader::new(OpenInput {
            chunks: input_rx,
            current: Cursor::new(Vec::new()),
        });
        let result = mcp::serve(
            input,
            BlockedOutput {
                entered: entered_tx,
                release: release_rx,
            },
        );
        done_tx.send(result).expect("client receives result");
    });
    input_tx.send(PING.to_vec()).expect("request");
    entered_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("response starts");
    release_tx.send(()).expect("fail output");
    let result = done_rx.recv_timeout(Duration::from_secs(2));
    // Close input even on failure, so a scoped-reader mutation can exit.
    drop(input_tx);
    thread.join().expect("session thread");
    assert_eq!(
        result
            .expect("serve must return before input closes")
            .expect_err("output fails")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
}
