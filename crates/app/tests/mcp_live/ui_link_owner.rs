//! The UI link's owner proof (#3995): a link file counts only while the
//! lock of its window is held, before the hello and after it. A stale
//! file is removed, an unproved one is skipped, and neither is dialed.
//! Provenance: our own tests over our own link; no HSMWorks material.

use super::ui_link::{cli_ui, fresh_dir, hold_lock, release_lock, write_entry};
use camcraft_app::mcp::link::{self, Entry};
use camcraft_app::mcp::link_owner::{self, Owner};
use camcraft_app::mcp::{link_call, LinkError, Select};

use serde_json::{json, Value};
use std::io::{BufRead, Write};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn false_entry(pid: u32, addr: std::net::SocketAddr) -> Entry {
    Entry {
        format: link::FORMAT,
        pid,
        transport: "tcp".into(),
        address: addr.to_string(),
        token: TOKEN.into(),
        project: None,
        app_version: "0.26.0".into(),
    }
}

/// True when a connection waits on `listener` (it never blocks).
fn was_dialed(listener: &std::net::TcpListener) -> bool {
    listener.set_nonblocking(true).expect("nonblocking");
    match listener.accept() {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(e) => panic!("accept: {e}"),
    }
}

/// #3995: a window crashed and left its file; a false server that read the
/// file holds the freed port and could prove the token. The free lock
/// shows that the window has ended: the CLI never dials, removes the stale
/// files and says so.
#[test]
fn a_crashed_windows_file_is_removed_and_its_port_is_never_dialed() {
    let dir = fresh_dir("crashed");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind false server");
    let entry = false_entry(4_000_000_001, listener.local_addr().expect("addr"));
    write_entry(&dir, &entry);
    // The lock file of the crashed window: its lock went with its process.
    release_lock(hold_lock(&dir, entry.pid));
    assert_eq!(link_owner::owner(&dir, entry.pid), Owner::Gone);

    let err = link_call(&dir, &Select::Only, "get_job", &json!({})).expect_err("no window runs");
    assert!(matches!(err, LinkError::NoWindow(_)), "{err:?}");
    assert_eq!(err.exit_code(), 3);
    assert!(err.to_string().contains("is stale: its window has ended"), "{err}");
    assert!(!was_dialed(&listener), "the false server got no connection, so no hello and no tool call");
    assert!(link::entries(&dir).is_empty(), "the stale link file is removed");
    assert!(!link_owner::lock_path(&dir, entry.pid).exists(), "the stale lock file is removed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #3995: a link file with no lock to test (an older build, or a file no
/// window wrote) is never dialed and the diagnostic says why. It is not a
/// silent fall-back to an unproved window.
#[test]
fn a_link_file_with_no_owner_proof_is_never_dialed() {
    let dir = fresh_dir("unproved");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind false server");
    let entry = false_entry(std::process::id(), listener.local_addr().expect("addr"));
    write_entry(&dir, &entry);

    let o = cli_ui(&dir, &["call", "get_job"]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("nothing proves that its window runs"), "{err}");
    assert!(err.contains("nothing ran"), "{err}");
    assert!(!was_dialed(&listener), "an unproved file is never dialed");
    assert_eq!(link::entries(&dir), vec![entry], "an unproved file is kept: its window may run");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #3995: a false server that knows the token and proves it correctly, but
/// whose window is not alive at the end of the hello, gets no tool call.
/// The CLI tests the lock again after the hello.
#[test]
fn a_false_server_that_proves_the_token_gets_no_tool_call_when_the_window_has_ended() {
    let dir = fresh_dir("proving-server");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind false server");
    let entry = false_entry(4_000_000_002, listener.local_addr().expect("addr"));
    write_entry(&dir, &entry);
    let lock = hold_lock(&dir, entry.pid);

    let false_server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("read hello");
        let hello: Value = serde_json::from_str(line.trim()).expect("hello json");
        let nonce = hello["nonce"].as_str().expect("nonce").to_owned();
        // The window ends during the hello; the false server answers with
        // the proof that it computes from the token in the file.
        release_lock(lock);
        writeln!(stream, "{}", link::hello_ok(&link::token_proof(TOKEN, &nonce, "server"))).expect("answer");
        stream.flush().expect("flush");
        line.clear();
        let n = reader.read_line(&mut line).unwrap_or(0);
        (n, line)
    });

    let err = link_call(&dir, &Select::Only, "get_job", &json!({})).expect_err("the window has ended");
    assert!(matches!(err, LinkError::NoWindow(_)), "{err:?}");
    let (n, line) = false_server.join().expect("server thread");
    assert_eq!(n, 0, "no tool call reached the false server: {line}");
    assert!(link::entries(&dir).is_empty(), "the stale link file is removed");
    let _ = std::fs::remove_dir_all(&dir);
}
