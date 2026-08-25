//! End-to-end client tests against the bundled mock server. These cover the
//! parts that are easy to get subtly wrong: message framing, matching responses
//! to the request that asked, and turning them into editor events.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use omv_lsp::{Client, Event, LspPosition, Notification, Request, ServerSpec};

const MOCK: &str = env!("CARGO_BIN_EXE_omv-mock-lsp");

fn start() -> (Client, Receiver<Event>) {
    let (tx, rx) = channel();
    let spec = ServerSpec {
        command: MOCK.to_string(),
        args: vec![],
    };
    let root = std::env::current_dir().expect("cwd");
    let client = Client::spawn("rust", &spec, &root, tx).expect("spawn mock server");
    (client, rx)
}

/// Wait for the first event matching `f`, ignoring unrelated traffic.
fn wait_for<T>(rx: &Receiver<Event>, mut f: impl FnMut(&Event) -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(event) => {
                if let Some(value) = f(&event) {
                    return value;
                }
                if let Event::Stopped { reason, .. } = &event {
                    panic!("server stopped early: {reason}");
                }
            }
            Err(_) => continue,
        }
    }
    panic!("timed out waiting for the expected event");
}

fn sample() -> PathBuf {
    std::env::current_dir().unwrap().join("src").join("lib.rs")
}

#[test]
fn completes_the_handshake_and_reports_the_server_name() {
    let (_client, rx) = start();
    let server = wait_for(&rx, |e| match e {
        Event::Ready { server, .. } => Some(server.clone()),
        _ => None,
    });
    assert_eq!(server, "mock-lsp");
}

#[test]
fn did_open_produces_diagnostics_for_the_right_file() {
    let (client, rx) = start();
    wait_for(&rx, |e| matches!(e, Event::Ready { .. }).then_some(()));

    let path = sample();
    client.notify(Notification::DidOpen {
        path: path.clone(),
        language_id: "rust".into(),
        version: 1,
        text: "fn main() {}\n".into(),
    });

    let (reported, diagnostics) = wait_for(&rx, |e| match e {
        Event::Diagnostics { path, diagnostics } => Some((path.clone(), diagnostics.clone())),
        _ => None,
    });
    // Round-tripping the path through a file:// URI must land back on the original.
    assert_eq!(reported, path);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "mock diagnostic");
    assert_eq!(diagnostics[0].range.start.line, 2);
}

#[test]
fn hover_response_is_matched_to_the_hover_request() {
    let (client, rx) = start();
    wait_for(&rx, |e| matches!(e, Event::Ready { .. }).then_some(()));

    client.request(Request::Hover {
        path: sample(),
        position: LspPosition {
            line: 0,
            character: 3,
        },
    });
    let text = wait_for(&rx, |e| match e {
        Event::Hover { text } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(text, "mock hover text");
}

#[test]
fn formatting_returns_edits() {
    let (client, rx) = start();
    wait_for(&rx, |e| matches!(e, Event::Ready { .. }).then_some(()));

    client.request(Request::Formatting {
        path: sample(),
        tab_size: 4,
    });
    let changes = wait_for(&rx, |e| match e {
        Event::Edits { changes } => Some(changes.clone()),
        _ => None,
    });
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].1[0].new_text, "FORMATTED");
}

#[test]
fn a_server_that_cannot_start_is_reported_promptly() {
    let (tx, rx) = channel();
    let spec = ServerSpec {
        command: "/nonexistent/definitely-not-a-server".into(),
        args: vec![],
    };
    let root = std::env::current_dir().unwrap();
    let client = Client::spawn("rust", &spec, &root, tx);

    // Spawning the thread succeeds; the failure surfaces as an event.
    if client.is_ok() {
        let reason = wait_for(&rx, |e| match e {
            Event::Stopped { reason, .. } => Some(reason.clone()),
            _ => None,
        });
        assert!(reason.contains("launching"), "unhelpful reason: {reason}");
    }
}
