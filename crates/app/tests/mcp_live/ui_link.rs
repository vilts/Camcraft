//! The UI link (#3751): `camcraft-cli --ui` finds a running window by
//! its discovery file and opens a session with the file's token. Pinned
//! here: no window is a clear error with exit 3 (never a headless run),
//! a wrong token is refused before a line reaches the app, a good
//! session carries a tool call to the window's inbox, and the file goes
//! when the window's inbox drops. A file whose window is not proved
//! alive by its lock is never dialed (#3995).
//! Provenance: our own tests over our own link; no HSMWorks material.

use camcraft_app::mcp::link::{self, Entry};
use camcraft_app::mcp::link_owner::{self, Owner};
use camcraft_app::mcp::{link_call, link_connect, Inbox, LinkError, Select};

use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub(super) fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-ui-link-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

pub(super) fn cli_ui(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .env(link::DIR_ENV, dir)
        .arg("--ui")
        .args(args)
        .output()
        .expect("the CLI binary runs")
}

/// Hold the lock of `pid` in `dir`, as a running window does (#3995).
pub(super) fn hold_lock(dir: &Path, pid: u32) -> std::fs::File {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(link_owner::lock_path(dir, pid))
        .expect("open the lock file");
    file.try_lock().expect("take the lock");
    file
}

/// End the window of a [`hold_lock`] file: release its lock at once, as
/// the OS does when a window process ends (#4041). A plain `drop` is not
/// enough here: on Unix a `flock` belongs to the open file, and a child
/// that another test thread is starting holds a copy of each descriptor
/// until its `exec`. Then the close does not release the lock, and the
/// CLI still reads the window as alive.
pub(super) fn release_lock(file: std::fs::File) {
    file.unlock().expect("release the lock");
}

pub(super) fn write_entry(dir: &Path, entry: &Entry) {
    let text = serde_json::to_string(entry).expect("entry json");
    std::fs::write(dir.join(format!("{}.json", entry.pid)), text).expect("write entry");
}

/// Answer the inbox's requests on this thread (the window's frame loop
/// stand-in) until `client` ends; returns the lines that reached it.
fn serve_until<T: Send + 'static>(inbox: &Inbox, client: std::thread::JoinHandle<T>) -> (T, Vec<String>) {
    let mut lines = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !client.is_finished() {
        assert!(Instant::now() < deadline, "the client did not finish");
        while let Some(req) = inbox.next() {
            let msg: Value = serde_json::from_str(req.line()).expect("json line");
            lines.push(req.line().to_owned());
            let reply = json!({
                "jsonrpc": "2.0", "id": msg["id"],
                "result": { "content": [], "structuredContent": { "echo": msg["params"]["name"] }, "isError": false }
            });
            req.answer(Some(reply.to_string()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    (client.join().expect("client thread"), lines)
}

#[test]
fn a_cli_call_with_no_window_says_so_and_exits_3() {
    let dir = fresh_dir("none");
    let o = cli_ui(&dir, &["call", "get_job"]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("no running CamCraft window"), "{err}");
    assert!(err.contains("nothing ran"), "{err}");
    assert!(o.stdout.is_empty(), "no headless answer on stdout");

    // A file a crashed window left behind: its socket does not answer.
    let stale = Entry {
        format: link::FORMAT,
        pid: 4_000_000_000,
        transport: "tcp".into(),
        address: "127.0.0.1:1".into(),
        token: "00".into(),
        project: None,
        app_version: "0".into(),
    };
    write_entry(&dir, &stale);
    let o = cli_ui(&dir, &["call", "get_job"]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("stale"), "{err}");

    // A window asked for by a pid that no file names.
    let o = cli_ui(&dir, &["--window", "123", "call", "get_job"]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("process id 123"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_ui_arguments_are_usage_errors() {
    let dir = fresh_dir("usage");
    for args in [&["call"][..], &["call", "get_job", "[1]"], &["call", "get_job", "{nope"], &["--window"], &["dance"]] {
        let o = cli_ui(&dir, args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    }
    let o = cli_ui(&dir, &["--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&o.stdout).contains("Exit: 0 stood, 1 refused, 2 usage, 3 no running window, 4 window busy"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_wrong_token_is_refused_and_the_window_goes_on_serving() {
    let dir = fresh_dir("token");
    let inbox = Inbox::link(&dir, Box::new(|| {})).expect("open the link");
    let entry = inbox.link_entry().expect("a link entry").clone();
    assert_eq!(link::entries(&dir), vec![entry.clone()], "the discovery file names the window");
    assert_eq!(entry.token.len(), 64);

    // The same window, but a file with a wrong token (its lock is held).
    let forged = fresh_dir("token-forged");
    write_entry(&forged, &Entry { token: "f".repeat(64), ..entry.clone() });
    let _forged_lock = hold_lock(&forged, entry.pid);
    let forged_dir = forged.clone();
    let client = std::thread::spawn(move || link_call(&forged_dir, &Select::Only, "get_job", &json!({})));
    let (result, lines) = serve_until(&inbox, client);
    let err = result.expect_err("a wrong token is refused");
    assert!(matches!(err, LinkError::Failed(_)), "{err:?}");
    assert!(err.to_string().contains("refused the link token"), "{err}");
    assert_eq!(err.exit_code(), 1);
    assert!(lines.is_empty(), "no line of a refused client reached the app: {lines:?}");

    // A hello that is not JSON at all is refused the same way.
    let wrong_hello = json!({ "camcraft_link": 1 }).to_string();
    assert!(link::hello_matches(wrong_hello.as_bytes(), &entry.token).is_none());
    assert!(link::hello_matches(b"GET / HTTP/1.1", &entry.token).is_none());
    assert!(link::hello_matches(link::hello_line(&entry.token, "test-nonce").as_bytes(), &entry.token).is_some());

    // The window still serves a client with the right token.
    let good_dir = dir.clone();
    let client = std::thread::spawn(move || link_call(&good_dir, &Select::Pid(std::process::id()), "get_job", &json!({})));
    let (result, lines) = serve_until(&inbox, client);
    let result = result.expect("the right token is served");
    assert_eq!(result["structuredContent"]["echo"], json!("get_job"));
    assert_eq!(lines.len(), 1, "one tools/call reached the window: {lines:?}");
    let line: Value = serde_json::from_str(&lines[0]).expect("json");
    assert_eq!(line["method"], json!("tools/call"));

    // The window's file and its lock file go with its inbox.
    assert_eq!(link_owner::owner(&dir, entry.pid), Owner::Alive, "the window holds its lock");
    drop(inbox);
    assert!(link::entries(&dir).is_empty(), "the discovery file is removed on close");
    assert!(!link_owner::lock_path(&dir, entry.pid).exists(), "the lock file is removed on close");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&forged);
}

#[test]
fn the_link_file_follows_the_open_project() {
    let dir = fresh_dir("project");
    let mut inbox = Inbox::link(&dir, Box::new(|| {})).expect("open the link");
    let project = dir.join("part.camproj");
    inbox.note_project(Some(&project));
    let entries = link::entries(&dir);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].project.as_deref(), Some(project.as_path()));
    assert!(Select::Project(project.clone()) != Select::Only);
    assert_eq!(Select::parse("42"), Select::Pid(42));
    assert_eq!(Select::parse("a/part.camproj"), Select::Project(PathBuf::from("a/part.camproj")));
    drop(inbox);
    let _ = std::fs::remove_dir_all(&dir);
}

fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<std::ffi::OsString> {
    let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    move |k: &str| pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.into())
}

#[test]
fn the_link_folder_is_the_users_own() {
    let tail = Path::new("camcraft").join("ui-link");
    // An absolute path is spelled per host, so each host checks its own.
    if cfg!(windows) {
        let roaming = r"C:\Users\u\AppData\Roaming";
        assert_eq!(link::dir_for("windows", lookup(&[("APPDATA", roaming)])), Some(Path::new(roaming).join(&tail)));
        assert_eq!(link::dir_for("windows", lookup(&[(link::DIR_ENV, r"D:\link")])), Some(PathBuf::from(r"D:\link")));
    } else {
        let run = "/run/user/1000";
        let both = [("XDG_RUNTIME_DIR", run), ("HOME", "/home/u")];
        assert_eq!(link::dir_for("linux", lookup(&both)), Some(Path::new(run).join(&tail)), "the runtime folder first");
        assert_eq!(link::dir_for("linux", lookup(&[("HOME", "/home/u")])), Some(Path::new("/home/u/.local/share").join(&tail)));
        assert_eq!(link::dir_for("macos", lookup(&[("HOME", "/Users/u")])), Some(Path::new("/Users/u/Library/Application Support").join(&tail)));
    }
    assert_eq!(link::dir_for("linux", lookup(&[])), None, "no home, no folder");
    assert_eq!(link::dir_for("linux", lookup(&[("HOME", "relative")])), None, "a relative home is refused");
    assert!(link::enabled(None));
    assert!(link::enabled(Some("on")));
    assert!(!link::enabled(Some(" OFF ")));
}

#[test]
fn false_server_on_stale_entry_is_refused_with_no_tool_calls_sent() {
    let dir = fresh_dir("false-server");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind false server");
    let addr = listener.local_addr().expect("addr");

    // A stale entry pointing to our false server port.
    let stale_entry = Entry {
        format: link::FORMAT,
        pid: std::process::id(),
        transport: "tcp".into(),
        address: addr.to_string(),
        token: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        project: None,
        app_version: "0.26.0".into(),
    };
    write_entry(&dir, &stale_entry);
    // A live owner, so the CLI reaches the hello and the proof check.
    let _lock = hold_lock(&dir, stale_entry.pid);

    // False server thread: accepts connection, sends generic HELLO_OK without proving token
    let false_server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("read hello");
        let _ = writeln!(stream, "{}", link::HELLO_OK);
        let _ = stream.flush();

        // Read to see if the CLI sends any subsequent tool calls
        line.clear();
        let bytes_read = reader.read_line(&mut line).unwrap_or(0);
        (bytes_read, line)
    });

    let result = link_call(&dir, &Select::Only, "get_job", &json!({}));
    assert!(result.is_err(), "false server must be rejected");

    let (bytes_read, line) = false_server.join().expect("server thread");
    assert_eq!(bytes_read, 0, "CLI must not send any tool call to unproven server: {line}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unauthenticated_client_holding_the_link_times_out_and_releases_it() {
    let dir = fresh_dir("hello-timeout");
    // Short hello deadline (150 ms) so the test is quick.
    let inbox = Inbox::link_with_timeouts(&dir, Box::new(|| {}), Duration::from_secs(60), Duration::from_millis(150))
        .expect("open the link");
    let entry = inbox.link_entry().expect("a link entry").clone();

    // Client 1 connects to the socket and sends no hello (#3873).
    let addr = entry.address.clone();
    let transport = entry.transport.clone();
    let client1 = std::thread::spawn(move || {
        let mut stream: Box<dyn Read + Send> = match transport.as_str() {
            "tcp" => Box::new(std::net::TcpStream::connect(&addr).expect("tcp connect")),
            #[cfg(unix)]
            "unix" => Box::new(std::os::unix::net::UnixStream::connect(&addr).expect("unix connect")),
            other => panic!("unknown transport: {other}"),
        };
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
    });

    std::thread::sleep(Duration::from_millis(30));

    // Client 2 connects with the token and gets the session after client 1 is dropped.
    let good_dir = dir.clone();
    let client2 = std::thread::spawn(move || link_call(&good_dir, &Select::Only, "get_job", &json!({})));

    let (result, lines) = serve_until(&inbox, client2);
    let result = result.expect("client 2 gets the session after client 1 times out");
    assert_eq!(result["structuredContent"]["echo"], json!("get_job"));
    assert_eq!(lines.len(), 1);

    client1.join().expect("client 1 thread finishes");
    drop(inbox);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_ui_call_during_a_slow_call_reports_busy_and_exits_4() {
    let dir = fresh_dir("busy");
    let inbox = Inbox::link(&dir, Box::new(|| {})).expect("open the link");

    // Client 1 connects and authenticates to occupy the session
    let dir1 = dir.clone();
    let (connected_tx, connected_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();

    let client1 = std::thread::spawn(move || {
        let stream = link_connect(&dir1, &Select::Only).expect("client 1 connects");
        let _ = connected_tx.send(());
        let _ = finish_rx.recv();
        drop(stream);
    });

    connected_rx.recv().expect("client 1 connected");
    std::thread::sleep(Duration::from_millis(50));

    // Client 2 attempts to call a tool via cli_ui; times out waiting on busy window
    let o = cli_ui(&dir, &["call", "get_job"]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(4), "{err}");
    assert!(err.contains("is busy with another call"), "{err}");

    let _ = finish_tx.send(());
    let _ = client1.join();
    drop(inbox);
    let _ = std::fs::remove_dir_all(&dir);
}

