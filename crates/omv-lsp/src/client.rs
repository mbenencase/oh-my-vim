use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender as StdSender;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use lsp_types::{Diagnostic, TextEdit};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::protocol::{Message, encode};
use crate::registry::ServerSpec;
use crate::uri::{path_to_uri, uri_to_path};

/// Positions here are LSP positions: zero-based line, and **UTF-16** character
/// offset. Callers convert from char columns (see `Buffer::utf16_column`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone)]
pub enum Request {
    Hover {
        path: PathBuf,
        position: LspPosition,
    },
    Definition {
        path: PathBuf,
        position: LspPosition,
    },
    References {
        path: PathBuf,
        position: LspPosition,
    },
    Rename {
        path: PathBuf,
        position: LspPosition,
        new_name: String,
    },
    Formatting {
        path: PathBuf,
        tab_size: u32,
    },
}

#[derive(Debug, Clone)]
pub enum Notify_ {
    DidOpen {
        path: PathBuf,
        language_id: String,
        version: i32,
        text: String,
    },
    DidChange {
        path: PathBuf,
        version: i32,
        text: String,
    },
    DidSave {
        path: PathBuf,
    },
    DidClose {
        path: PathBuf,
    },
}

#[derive(Debug, Clone)]
enum Command {
    Request(Request),
    Notify(Notify_),
    Shutdown,
}

/// Anything the server tells us, flattened into things the editor can act on.
#[derive(Debug, Clone)]
pub enum Event {
    Ready {
        language: String,
        server: String,
    },
    Diagnostics {
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
    },
    Hover {
        text: String,
    },
    /// A jump target. `None` when the server had no definition to offer.
    Definition {
        path: PathBuf,
        position: LspPosition,
    },
    References {
        locations: Vec<(PathBuf, LspPosition)>,
    },
    /// Edits to apply, grouped by file. Covers both formatting and rename.
    Edits {
        changes: Vec<(PathBuf, Vec<TextEdit>)>,
    },
    /// `window/showMessage` and friends.
    Message {
        text: String,
    },
    Error {
        text: String,
    },
    Stopped {
        language: String,
        reason: String,
    },
}

/// What a pending request id means, so the reader knows how to shape the reply.
#[derive(Debug, Clone, Copy)]
enum Pending {
    Initialize,
    Hover,
    Definition,
    References,
    Rename,
    Formatting,
}

/// A running language server. Dropping it shuts the server down.
pub struct Client {
    pub language: String,
    tx: UnboundedSender<Command>,
}

impl Client {
    /// Start `spec` for `language`, rooted at `root`.
    ///
    /// Runs a single-threaded tokio runtime on its own thread; the editor stays
    /// synchronous and only ever sees `events`.
    pub fn spawn(
        language: &str,
        spec: &ServerSpec,
        root: &Path,
        events: StdSender<Event>,
    ) -> Result<Client> {
        let (tx, rx) = unbounded_channel::<Command>();
        let language_owned = language.to_string();
        let spec = spec.clone();
        let root = root.to_path_buf();

        std::thread::Builder::new()
            .name(format!("omv-lsp-{language}"))
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = events.send(Event::Error {
                            text: format!("lsp runtime: {e}"),
                        });
                        return;
                    }
                };
                let language = language_owned.clone();
                runtime.block_on(async move {
                    if let Err(e) = run(&language, &spec, &root, rx, events.clone()).await {
                        let _ = events.send(Event::Stopped {
                            language: language.clone(),
                            reason: e.to_string(),
                        });
                    }
                });
            })
            .context("spawning the LSP thread")?;

        Ok(Client {
            language: language.to_string(),
            tx,
        })
    }

    pub fn request(&self, request: Request) {
        let _ = self.tx.send(Command::Request(request));
    }

    pub fn notify(&self, notification: Notify_) {
        let _ = self.tx.send(Command::Notify(notification));
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.shutdown();
    }
}

type PendingMap = Arc<Mutex<HashMap<i64, Pending>>>;

async fn run(
    language: &str,
    spec: &ServerSpec,
    root: &Path,
    mut rx: UnboundedReceiver<Command>,
    events: StdSender<Event>,
) -> Result<()> {
    let mut child: Child = TokioCommand::new(&spec.command)
        .args(&spec.args)
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("launching `{}` — is it on PATH?", spec.command))?;

    let mut stdin = child.stdin.take().context("server stdin")?;
    let stdout = child.stdout.take().context("server stdout")?;
    let stderr = child.stderr.take().context("server stderr")?;

    let (out_tx, mut out_rx) = unbounded_channel::<Message>();
    let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
    let ready = Arc::new(Notify::new());

    // Writer: the only thing that touches the server's stdin.
    tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            let Ok(bytes) = encode(&message) else {
                continue;
            };
            if stdin.write_all(&bytes).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
    });

    // A server that fills its stderr pipe and is never drained will block, so
    // drain it unconditionally and keep the tail for the exit message.
    let last_stderr = Arc::new(Mutex::new(String::new()));
    {
        let last_stderr = Arc::clone(&last_stderr);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                *last_stderr.lock().unwrap() = line;
            }
        });
    }

    // Reader: parses framed messages and turns them into editor events.
    {
        let pending = Arc::clone(&pending);
        let events = events.clone();
        let out_tx = out_tx.clone();
        let ready = Arc::clone(&ready);
        let language = language.to_string();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader).await {
                    Ok(Some(message)) => {
                        handle_message(message, &pending, &events, &out_tx, &ready, &language)
                    }
                    Ok(None) => break, // clean EOF: the server exited
                    Err(e) => {
                        let _ = events.send(Event::Error {
                            text: format!("lsp read: {e}"),
                        });
                        break;
                    }
                }
            }
        });
    }

    // Handshake. Commands queued before this completes simply wait in `rx`.
    let root_uri = path_to_uri(root);
    pending.lock().unwrap().insert(0, Pending::Initialize);
    out_tx.send(Message::request(
        0,
        "initialize",
        initialize_params(root_uri.as_ref()),
    ))?;

    // Watch for the child dying during the handshake too. A server that exits
    // immediately (missing component, bad args, wrong cwd) must be reported now,
    // with its stderr — not after a 30-second wait for a reply that never comes.
    tokio::select! {
        _ = ready.notified() => {}
        status = child.wait() => {
            let tail = last_stderr.lock().unwrap().clone();
            let detail = match status {
                Ok(s) => format!("exited with {s}"),
                Err(e) => format!("could not be waited on: {e}"),
            };
            if tail.is_empty() {
                anyhow::bail!("`{}` {detail} during startup", spec.command);
            }
            anyhow::bail!("`{}` {detail} during startup: {tail}", spec.command);
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
            anyhow::bail!("`{}` did not answer initialize within 30s", spec.command);
        }
    }
    out_tx.send(Message::notification("initialized", json!({})))?;

    let mut next_id: i64 = 1;
    loop {
        tokio::select! {
            command = rx.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Shutdown => break,
                    Command::Notify(n) => {
                        if let Some(message) = notification_message(&n) {
                            let _ = out_tx.send(message);
                        }
                    }
                    Command::Request(r) => {
                        let id = next_id;
                        next_id += 1;
                        if let Some((message, kind)) = request_message(id, &r) {
                            pending.lock().unwrap().insert(id, kind);
                            let _ = out_tx.send(message);
                        }
                    }
                }
            }
            status = child.wait() => {
                let tail = last_stderr.lock().unwrap().clone();
                let reason = match status {
                    Ok(s) if tail.is_empty() => format!("exited with {s}"),
                    Ok(s) => format!("exited with {s}: {tail}"),
                    Err(e) => format!("wait failed: {e}"),
                };
                let _ = events.send(Event::Stopped { language: language.to_string(), reason });
                return Ok(());
            }
        }
    }

    let _ = out_tx.send(Message::request(next_id, "shutdown", Value::Null));
    let _ = out_tx.send(Message::notification("exit", Value::Null));
    // Give it a moment to leave on its own before `kill_on_drop` takes over.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
    Ok(())
}

/// Read one `Content-Length` framed message. `Ok(None)` means clean EOF.
async fn read_message<R>(reader: &mut BufReader<R>) -> Result<Option<Message>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut content_length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // end of headers
        }
        if let Some(value) = trimmed
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, v)| v.trim())
        {
            content_length = Some(value.parse().context("bad Content-Length")?);
        }
    }
    let length = content_length.context("message with no Content-Length header")?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    Ok(Some(
        serde_json::from_slice(&body).context("malformed JSON-RPC body")?,
    ))
}

fn handle_message(
    message: Message,
    pending: &PendingMap,
    events: &StdSender<Event>,
    out_tx: &UnboundedSender<Message>,
    ready: &Arc<Notify>,
    language: &str,
) {
    // Server → client request: must be answered or some servers stall.
    if let (Some(method), Some(id)) = (message.method.as_deref(), message.id.clone()) {
        match method {
            "workspace/configuration" => {
                let _ = out_tx.send(Message::response(id, json!([{}])));
            }
            _ => {
                let _ = out_tx.send(Message::response(id, Value::Null));
            }
        }
        return;
    }

    // Server → client notification.
    if let Some(method) = message.method.as_deref() {
        match method {
            "textDocument/publishDiagnostics" => {
                if let Some(params) = message.params
                    && let Ok(p) =
                        serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params)
                    && let Some(path) = uri_to_path(&p.uri)
                {
                    let _ = events.send(Event::Diagnostics {
                        path,
                        diagnostics: p.diagnostics,
                    });
                }
            }
            "window/showMessage" | "window/logMessage" => {
                if let Some(text) = message
                    .params
                    .as_ref()
                    .and_then(|p| p.get("message"))
                    .and_then(Value::as_str)
                {
                    let _ = events.send(Event::Message {
                        text: text.to_string(),
                    });
                }
            }
            _ => {}
        }
        return;
    }

    // Response to one of ours.
    if !message.is_response() {
        return;
    }
    let Some(id) = message.id.as_ref().and_then(Value::as_i64) else {
        return;
    };
    let Some(kind) = pending.lock().unwrap().remove(&id) else {
        return;
    };

    if let Some(error) = message.error {
        let _ = events.send(Event::Error {
            text: format!("lsp: {}", error.message),
        });
        if matches!(kind, Pending::Initialize) {
            ready.notify_one(); // unblock the handshake rather than hanging for 30s
        }
        return;
    }
    let result = message.result.unwrap_or(Value::Null);

    match kind {
        Pending::Initialize => {
            let server = result
                .get("serverInfo")
                .and_then(|i| i.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(language)
                .to_string();
            let _ = events.send(Event::Ready {
                language: language.to_string(),
                server,
            });
            ready.notify_one();
        }
        Pending::Hover => {
            if let Ok(Some(hover)) = serde_json::from_value::<Option<lsp_types::Hover>>(result) {
                let text = hover_text(&hover.contents);
                if !text.trim().is_empty() {
                    let _ = events.send(Event::Hover { text });
                }
            }
        }
        Pending::Definition => {
            if let Some((path, position)) = first_location(&result) {
                let _ = events.send(Event::Definition { path, position });
            }
        }
        Pending::References => {
            let locations = all_locations(&result);
            let _ = events.send(Event::References { locations });
        }
        Pending::Rename => {
            if let Ok(edit) = serde_json::from_value::<lsp_types::WorkspaceEdit>(result) {
                let _ = events.send(Event::Edits {
                    changes: workspace_edit_changes(edit),
                });
            }
        }
        Pending::Formatting => {
            if let Ok(Some(edits)) = serde_json::from_value::<Option<Vec<TextEdit>>>(result) {
                // Formatting replies carry no URI; the caller knows it was the
                // current buffer, so an empty path means "the file you asked about".
                let _ = events.send(Event::Edits {
                    changes: vec![(PathBuf::new(), edits)],
                });
            }
        }
    }
}

fn hover_text(contents: &lsp_types::HoverContents) -> String {
    use lsp_types::{HoverContents, MarkedString};
    fn marked(m: &MarkedString) -> String {
        match m {
            MarkedString::String(s) => s.clone(),
            MarkedString::LanguageString(ls) => ls.value.clone(),
        }
    }
    match contents {
        HoverContents::Scalar(m) => marked(m),
        HoverContents::Array(ms) => ms.iter().map(marked).collect::<Vec<_>>().join("\n\n"),
        HoverContents::Markup(markup) => markup.value.clone(),
    }
}

fn location_from(value: &Value) -> Option<(PathBuf, LspPosition)> {
    let uri = value.get("uri").or_else(|| value.get("targetUri"))?;
    let uri: lsp_types::Uri = serde_json::from_value(uri.clone()).ok()?;
    let range = value
        .get("range")
        .or_else(|| value.get("targetSelectionRange"))?;
    let start = range.get("start")?;
    Some((
        uri_to_path(&uri)?,
        LspPosition {
            line: start.get("line")?.as_u64()? as u32,
            character: start.get("character")?.as_u64()? as u32,
        },
    ))
}

fn first_location(result: &Value) -> Option<(PathBuf, LspPosition)> {
    match result {
        Value::Array(items) => items.iter().find_map(location_from),
        Value::Object(_) => location_from(result),
        _ => None,
    }
}

fn all_locations(result: &Value) -> Vec<(PathBuf, LspPosition)> {
    match result {
        Value::Array(items) => items.iter().filter_map(location_from).collect(),
        Value::Object(_) => location_from(result).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn workspace_edit_changes(edit: lsp_types::WorkspaceEdit) -> Vec<(PathBuf, Vec<TextEdit>)> {
    let mut out = Vec::new();
    if let Some(changes) = edit.changes {
        for (uri, edits) in changes {
            if let Some(path) = uri_to_path(&uri) {
                out.push((path, edits));
            }
        }
    }
    if let Some(lsp_types::DocumentChanges::Edits(docs)) = edit.document_changes {
        for doc in docs {
            if let Some(path) = uri_to_path(&doc.text_document.uri) {
                let edits = doc
                    .edits
                    .into_iter()
                    .map(|e| match e {
                        lsp_types::OneOf::Left(edit) => edit,
                        lsp_types::OneOf::Right(annotated) => annotated.text_edit,
                    })
                    .collect();
                out.push((path, edits));
            }
        }
    }
    out
}

fn notification_message(n: &Notify_) -> Option<Message> {
    Some(match n {
        Notify_::DidOpen {
            path,
            language_id,
            version,
            text,
        } => Message::notification(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": path_to_uri(path)?, "languageId": language_id,
                "version": version, "text": text,
            }}),
        ),
        // Full-document sync: simple and correct. Incremental sync is a later
        // optimisation that needs the rope's change list plumbed through.
        Notify_::DidChange {
            path,
            version,
            text,
        } => Message::notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": path_to_uri(path)?, "version": version },
                "contentChanges": [{ "text": text }],
            }),
        ),
        Notify_::DidSave { path } => Message::notification(
            "textDocument/didSave",
            json!({ "textDocument": { "uri": path_to_uri(path)? } }),
        ),
        Notify_::DidClose { path } => Message::notification(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": path_to_uri(path)? } }),
        ),
    })
}

fn request_message(id: i64, r: &Request) -> Option<(Message, Pending)> {
    let doc = |path: &PathBuf| -> Option<Value> { Some(json!({ "uri": path_to_uri(path)? })) };
    let at = |path: &PathBuf, p: &LspPosition| -> Option<Value> {
        Some(json!({
            "textDocument": doc(path)?,
            "position": { "line": p.line, "character": p.character },
        }))
    };
    Some(match r {
        Request::Hover { path, position } => (
            Message::request(id, "textDocument/hover", at(path, position)?),
            Pending::Hover,
        ),
        Request::Definition { path, position } => (
            Message::request(id, "textDocument/definition", at(path, position)?),
            Pending::Definition,
        ),
        Request::References { path, position } => {
            let mut params = at(path, position)?;
            params["context"] = json!({ "includeDeclaration": false });
            (
                Message::request(id, "textDocument/references", params),
                Pending::References,
            )
        }
        Request::Rename {
            path,
            position,
            new_name,
        } => {
            let mut params = at(path, position)?;
            params["newName"] = json!(new_name);
            (
                Message::request(id, "textDocument/rename", params),
                Pending::Rename,
            )
        }
        Request::Formatting { path, tab_size } => (
            Message::request(
                id,
                "textDocument/formatting",
                json!({
                    "textDocument": doc(path)?,
                    "options": { "tabSize": tab_size, "insertSpaces": true },
                }),
            ),
            Pending::Formatting,
        ),
    })
}

fn initialize_params(root_uri: Option<&lsp_types::Uri>) -> Value {
    json!({
        "processId": std::process::id(),
        "rootUri": root_uri,
        "workspaceFolders": root_uri.map(|u| json!([{ "uri": u, "name": "root" }])),
        "capabilities": {
            "general": {
                // We send UTF-16 columns; be explicit rather than relying on the default.
                "positionEncodings": ["utf-16"]
            },
            "textDocument": {
                "synchronization": { "didSave": true, "dynamicRegistration": false },
                "hover": { "contentFormat": ["markdown", "plaintext"] },
                "definition": { "linkSupport": true },
                "references": {},
                "rename": { "prepareSupport": false },
                "formatting": {},
                "publishDiagnostics": { "relatedInformation": false },
            },
            "workspace": { "workspaceFolders": true, "configuration": true },
        },
    })
}
