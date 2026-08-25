//! A minimal language server used as a test fixture.
//!
//! It implements just enough of the protocol to exercise the client's framing,
//! request/response correlation, and event mapping without depending on a real
//! language server being installed. Not useful for editing anything.

use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();

    while let Some(message) = read_message(&mut reader) {
        let method = message["method"].as_str().unwrap_or("");
        let id = message.get("id").cloned();

        match method {
            "initialize" => {
                send(
                    &mut writer,
                    &serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {
                            "capabilities": { "hoverProvider": true },
                            "serverInfo": { "name": "mock-lsp", "version": "0.1.0" }
                        }
                    }),
                );
            }
            "textDocument/didOpen" => {
                let uri = message["params"]["textDocument"]["uri"].clone();
                send(
                    &mut writer,
                    &serde_json::json!({
                        "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                        "params": { "uri": uri, "diagnostics": [{
                            "range": {"start": {"line": 2, "character": 4},
                                      "end":   {"line": 2, "character": 9}},
                            "severity": 1, "message": "mock diagnostic"
                        }]}
                    }),
                );
            }
            "textDocument/hover" => {
                send(
                    &mut writer,
                    &serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "contents": { "kind": "markdown", "value": "mock hover text" } }
                    }),
                );
            }
            "textDocument/formatting" => {
                send(
                    &mut writer,
                    &serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": [{
                            "range": {"start": {"line": 0, "character": 0},
                                      "end":   {"line": 0, "character": 5}},
                            "newText": "FORMATTED"
                        }]
                    }),
                );
            }
            "shutdown" => {
                send(
                    &mut writer,
                    &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": null}),
                );
            }
            "exit" => return,
            _ => {
                // Every request must be answered, even ones we don't implement.
                if id.is_some() {
                    send(
                        &mut writer,
                        &serde_json::json!({"jsonrpc":"2.0","id":id,"result":null}),
                    );
                }
            }
        }
    }
}

fn read_message(reader: &mut impl BufRead) -> Option<serde_json::Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0u8; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn send(writer: &mut impl Write, value: &serde_json::Value) {
    let body = serde_json::to_vec(value).expect("serialise");
    let _ = write!(writer, "Content-Length: {}\r\n\r\n", body.len());
    let _ = writer.write_all(&body);
    let _ = writer.flush();
}
