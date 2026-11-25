//! texlab over JSON-RPC (lsp.ts): the open files are kept in sync, and texlab answers completion
//! of commands the local list doesn't know, hover notes, go to definition and find references.
//! The process and its `Content-Length` framing live in lemmata-shell; this side speaks JSON.

use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Off,
    /// Starting (or downloading on first use).
    Starting,
    Ready,
    Failed(String),
}

/// A completion candidate from texlab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
    pub filter_text: Option<String>,
    pub kind: Option<u64>,
}

/// A live diagnostic from texlab: 1-based line, optional 0-based UTF-16 column span, severity, message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub start_col: Option<usize>,
    pub end_col: Option<usize>,
    pub severity: Severity,
    pub message: String,
}

/// One text replacement from an LSP `CodeAction`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub start: (usize, usize),
    pub end: (usize, usize),
    pub new_text: String,
}

/// A quick fix offered by texlab (`textDocument/codeAction`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeAction {
    pub title: String,
    /// Edits grouped by project-relative file path.
    pub edits: Vec<(String, Vec<TextEdit>)>,
}

/// What a request was for, so its answer goes to the right place.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Initialize,
    /// Completion at this file and cursor (chars).
    Complete {
        path: String,
        head: usize,
        from: usize,
    },
    Hover {
        path: String,
        version: u64,
        line: usize,
        col: usize,
    },
    CodeActions {
        path: String,
        version: u64,
        line: usize,
    },
    Definition,
    References,
}

/// An answer the workspace acts on.
#[derive(Debug, PartialEq)]
pub enum Answer {
    Ready,
    Completion {
        path: String,
        head: usize,
        from: usize,
        items: Vec<CompletionItem>,
    },
    Hover {
        path: String,
        version: u64,
        line: usize,
        col: usize,
        text: String,
    },
    CodeActions {
        path: String,
        version: u64,
        line: usize,
        actions: Vec<CodeAction>,
    },
    /// Places in the project: path relative to the root, 1-based line, and 0-based column (UTF-16).
    Locations {
        references: bool,
        places: Vec<(String, usize, usize)>,
    },
    /// texlab's problems for this file changed (see `Lsp::diagnostics`).
    Diagnostics(String),
}

/// How serious texlab finds a problem (LSP's 1 to 3; hints such as "Unused label" are left out).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

pub struct Lsp {
    pub state: State,
    pub generation: Option<u64>,
    next_id: i64,
    pending: HashMap<i64, (Pending, std::time::Instant)>,
    /// Open documents and the editor version texlab last saw.
    opened: HashMap<String, (i64, u64)>,
    root: String,
    /// texlab takes changed ranges rather than the whole text (its `textDocumentSync`).
    incremental: bool,
    /// Problems texlab sees as the text is typed, without a build: unclosed braces, undefined
    /// references and citations, duplicate labels.
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
}

/// An open document for `Lsp::sync`: path, editor version, its text on demand, and the changes
/// since the last sync (`None`: only the whole text will do).
pub type SyncDoc<'a> = (
    String,
    u64,
    &'a dyn Fn() -> String,
    Option<Vec<crate::editor::buffer::LspEdit>>,
);

/// Project-relative path ↔ file:// URI.
pub fn to_uri(root: &str, rel: &str) -> String {
    let abs = format!("{}/{rel}", root.trim_end_matches(['/', '\\'])).replace('\\', "/");
    let with_slash = if abs.starts_with('/') {
        abs
    } else {
        format!("/{abs}")
    };
    let mut out = String::from("file://");
    for b in with_slash.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn from_uri(root: &str, uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let mut bytes = Vec::new();
    let raw = rest.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%'
            && i + 2 < raw.len()
            && let Ok(b) = u8::from_str_radix(std::str::from_utf8(&raw[i + 1..i + 3]).ok()?, 16)
        {
            bytes.push(b);
            i += 3;
            continue;
        }
        bytes.push(raw[i]);
        i += 1;
    }
    let mut path = String::from_utf8(bytes).ok()?;
    // Windows drive paths come as /C:/…
    if path.len() > 3 && path.as_bytes()[0] == b'/' && path.as_bytes()[2] == b':' {
        path.remove(0);
    }
    let base = format!("{}/", root.replace('\\', "/").trim_end_matches('/'));
    let norm = path.replace('\\', "/");
    // Only match case-insensitively on Windows/macOS or Windows drive paths, comparing
    // char-by-char so case-expanding Unicode (e.g. `İ`, `ẞ`) can never panic on byte offsets.
    let case_insensitive =
        cfg!(any(windows, target_os = "macos")) || (base.len() >= 2 && base.as_bytes()[1] == b':');
    if !case_insensitive {
        return norm.strip_prefix(&base).map(str::to_owned);
    }
    let mut norm_iter = norm.char_indices();
    for bc in base.chars() {
        let (_, nc) = norm_iter.next()?;
        if !bc.to_lowercase().eq(nc.to_lowercase()) {
            return None;
        }
    }
    let rest_start = norm_iter.next().map_or(norm.len(), |(b, _)| b);
    Some(norm[rest_start..].to_owned())
}

/// LSP columns count UTF-16 units; the editor counts chars.
pub fn utf16_col(line: &str, chars: usize) -> usize {
    line.chars().take(chars).map(char::len_utf16).sum()
}

/// Inverse of `utf16_col`: UTF-16 code units on `line` to a 0-based char column.
pub fn utf16_to_char(line: &str, utf16: usize) -> usize {
    let mut units = 0usize;
    for (i, c) in line.chars().enumerate() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    line.chars().count()
}

fn language(path: &str) -> &'static str {
    if path.ends_with(".bib") {
        "bibtex"
    } else {
        "latex"
    }
}

/// texlab's Markdown as plain text for hover cards and completion docs: strips headings,
/// fences, emphasis, inline links (`[text](url)` -> `text`), list bullets, and table dividers.
pub fn plain(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for raw_line in md.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            continue;
        }
        // Skip Markdown pipe-table separator rows like `| --- | :---: |`.
        if trimmed.contains('-')
            && trimmed.contains('|')
            && trimmed
                .chars()
                .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
        {
            continue;
        }
        let mut line = raw_line.trim_start_matches('#').trim_start();
        if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            out.push_str("• ");
            line = rest;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\'
                && let Some(&next) = chars.get(i + 1)
                && next.is_ascii_punctuation()
            {
                out.push(next);
                i += 2;
                continue;
            }
            // Inline Markdown link: `[label](url)` -> `label`.
            if c == '['
                && let Some(close_br) = chars[i + 1..].iter().position(|&ch| ch == ']')
            {
                let close_br = i + 1 + close_br;
                if chars.get(close_br + 1) == Some(&'(')
                    && let Some(close_pa) = chars[close_br + 2..].iter().position(|&ch| ch == ')')
                {
                    let label: String = chars[i + 1..close_br].iter().collect();
                    out.push_str(&plain(&label));
                    i = close_br + 2 + close_pa + 1;
                    continue;
                }
            }
            match c {
                '*' | '`' => {}
                '_' if out.ends_with([' ', '_'])
                    || out.is_empty()
                    || chars.get(i + 1) == Some(&'_') => {}
                _ => out.push(c),
            }
            i += 1;
        }
        out.push('\n');
    }
    out.trim().to_owned()
}

/// Files texlab is told about.
pub fn wants(path: &str) -> bool {
    [".tex", ".bib", ".sty", ".cls"]
        .iter()
        .any(|e| path.ends_with(e))
}

impl Lsp {
    pub fn new(root: &str) -> Self {
        Self {
            state: State::Off,
            generation: None,
            next_id: 1,
            pending: HashMap::new(),
            opened: HashMap::new(),
            root: root.to_owned(),
            incremental: false,
            diagnostics: HashMap::new(),
        }
    }

    fn request(&mut self, method: &str, params: Value, what: Pending) -> String {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, (what, std::time::Instant::now()));
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
    }

    fn notify(method: &str, params: Value) -> String {
        json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
    }

    /// Requests texlab has not answered for this long are given up on, and returned.
    pub fn expire(&mut self, after: std::time::Duration) -> Vec<Pending> {
        let stale: Vec<i64> = self
            .pending
            .iter()
            .filter(|(_, (_, at))| at.elapsed() > after)
            .map(|(id, _)| *id)
            .collect();
        stale
            .into_iter()
            .filter_map(|id| self.pending.remove(&id).map(|(what, _)| what))
            .collect()
    }

    /// The first message, once the process is up. A restarted texlab knows none of the documents
    /// or requests of the one before.
    pub fn initialize(&mut self) -> String {
        self.opened.clear();
        self.diagnostics.clear();
        self.pending.clear();
        self.incremental = false;
        let root = to_uri(&self.root, "");
        let root = root.trim_end_matches('/').to_owned();
        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root,
                "workspaceFolders": [{ "uri": root, "name": "project" }],
                "capabilities": {
                    "textDocument": {
                        "synchronization": { "didSave": false },
                        "completion": { "completionItem": { "snippetSupport": false, "documentationFormat": ["plaintext", "markdown"] } },
                        "hover": { "contentFormat": ["plaintext", "markdown"] },
                        "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": ["quickfix"] } } },
                        "definition": {},
                        "references": {},
                    },
                    "workspace": { "workspaceFolders": true },
                },
            }),
            Pending::Initialize,
        )
    }

    /// didOpen / didChange for open files whose text texlab hasn't seen; didClose for closed ones.
    pub fn sync(&mut self, docs: &[SyncDoc]) -> Vec<String> {
        if self.state != State::Ready {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (path, version, text, edits) in docs {
            if !wants(path) {
                continue;
            }
            let uri = to_uri(&self.root, path);
            match self.opened.get_mut(path) {
                None => {
                    self.opened.insert(path.clone(), (1, *version));
                    out.push(Self::notify(
                        "textDocument/didOpen",
                        json!({ "textDocument": { "uri": uri, "languageId": language(path), "version": 1, "text": text() } }),
                    ));
                }
                Some((v, seen)) if *seen != *version => {
                    *v += 1;
                    *seen = *version;
                    let changes: Vec<Value> = match edits {
                        Some(e) if self.incremental && !e.is_empty() => e
                            .iter()
                            .map(|e| {
                                let pos = |(line, character): (usize, usize)| {
                                    json!({ "line": line, "character": character })
                                };
                                json!({
                                    "range": { "start": pos(e.start), "end": pos(e.end) },
                                    "text": e.text,
                                })
                            })
                            .collect(),
                        _ => vec![json!({ "text": text() })],
                    };
                    out.push(Self::notify(
                        "textDocument/didChange",
                        json!({ "textDocument": { "uri": uri, "version": *v }, "contentChanges": changes }),
                    ));
                }
                Some(_) => {}
            }
        }
        let open: Vec<&String> = docs.iter().map(|d| &d.0).collect();
        let closed: Vec<String> = self
            .opened
            .keys()
            .filter(|p| !open.contains(p))
            .cloned()
            .collect();
        for p in closed {
            self.opened.remove(&p);
            self.diagnostics.remove(&p);
            out.push(Self::notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": to_uri(&self.root, &p) } }),
            ));
        }
        out
    }

    fn position(&self, path: &str, line: usize, utf16: usize) -> Value {
        json!({ "textDocument": { "uri": to_uri(&self.root, path) }, "position": { "line": line, "character": utf16 } })
    }

    pub fn complete(
        &mut self,
        path: &str,
        line: usize,
        utf16: usize,
        head: usize,
        from: usize,
    ) -> Option<String> {
        (self.state == State::Ready && self.opened.contains_key(path)).then(|| {
            let p = self.position(path, line, utf16);
            self.request(
                "textDocument/completion",
                p,
                Pending::Complete {
                    path: path.into(),
                    head,
                    from,
                },
            )
        })
    }

    pub fn hover(
        &mut self,
        path: &str,
        version: u64,
        line: usize,
        utf16: usize,
        col: usize,
    ) -> Option<String> {
        (self.state == State::Ready && self.opened.contains_key(path)).then(|| {
            let p = self.position(path, line, utf16);
            self.request(
                "textDocument/hover",
                p,
                Pending::Hover {
                    path: path.into(),
                    version,
                    line,
                    col,
                },
            )
        })
    }

    pub fn code_actions(
        &mut self,
        path: &str,
        version: u64,
        line: usize,
        start_utf16: usize,
        end_utf16: usize,
    ) -> Option<String> {
        (self.state == State::Ready && self.opened.contains_key(path)).then(|| {
            let params = json!({
                "textDocument": { "uri": to_uri(&self.root, path) },
                "range": {
                    "start": { "line": line, "character": start_utf16 },
                    "end": { "line": line, "character": end_utf16 },
                },
                "context": {
                    "diagnostics": [],
                    "only": ["quickfix"],
                }
            });
            self.request(
                "textDocument/codeAction",
                params,
                Pending::CodeActions {
                    path: path.into(),
                    version,
                    line,
                },
            )
        })
    }

    pub fn definition(&mut self, path: &str, line: usize, utf16: usize) -> Option<String> {
        (self.state == State::Ready && self.opened.contains_key(path)).then(|| {
            let p = self.position(path, line, utf16);
            self.request("textDocument/definition", p, Pending::Definition)
        })
    }

    pub fn references(&mut self, path: &str, line: usize, utf16: usize) -> Option<String> {
        (self.state == State::Ready && self.opened.contains_key(path)).then(|| {
            let mut p = self.position(path, line, utf16);
            p["context"] = json!({ "includeDeclaration": true });
            self.request("textDocument/references", p, Pending::References)
        })
    }

    fn on_diagnostics(&mut self, params: &Value) -> Option<Answer> {
        let path = from_uri(&self.root, params["uri"].as_str()?)?;
        // Only open files: a closed one's problems would never be cleared.
        let (version, _) = self.opened.get(&path)?;
        // Computed for the text before the latest edit: the lines would be off until the next run.
        if params["version"].as_i64().is_some_and(|v| v != *version) {
            return None;
        }
        let mut list = params["diagnostics"]
            .as_array()?
            .iter()
            .filter_map(|d| {
                let severity = match d["severity"].as_u64().unwrap_or(1) {
                    1 => Severity::Error,
                    2 => Severity::Warning,
                    3 => Severity::Info,
                    _ => return None,
                };
                let start_line = d["range"]["start"]["line"].as_u64()? as usize;
                let end_line = d["range"]["end"]["line"]
                    .as_u64()
                    .map(|v| v as usize)
                    .unwrap_or(start_line);
                let start_col = d["range"]["start"]["character"]
                    .as_u64()
                    .map(|v| v as usize);
                let end_col = if end_line == start_line {
                    d["range"]["end"]["character"].as_u64().map(|v| v as usize)
                } else {
                    None
                };
                Some(Diagnostic {
                    line: start_line + 1,
                    start_col,
                    end_col,
                    severity,
                    message: d["message"].as_str()?.to_owned(),
                })
            })
            .collect::<Vec<_>>();
        list.sort_by(|a, b| {
            a.line
                .cmp(&b.line)
                .then_with(|| a.severity.cmp(&b.severity))
                .then_with(|| a.message.cmp(&b.message))
        });
        list.dedup();
        self.diagnostics.insert(path.clone(), list);
        Some(Answer::Diagnostics(path))
    }

    /// One message from texlab: an answer to act on, and anything to send back.
    pub fn on_message(&mut self, body: &str) -> (Option<Answer>, Vec<String>) {
        let Ok(msg) = serde_json::from_str::<Value>(body) else {
            return (None, Vec::new());
        };
        // A request from the server (workspace/configuration and the like): answer it plainly.
        if msg.get("method").is_some() {
            if let Some(id) = msg.get("id") {
                let result = if msg["method"] == "workspace/configuration" {
                    let n = msg["params"]["items"].as_array().map_or(0, Vec::len);
                    Value::Array(vec![Value::Null; n])
                } else {
                    Value::Null
                };
                return (
                    None,
                    vec![json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()],
                );
            }
            if msg["method"] == "textDocument/publishDiagnostics" {
                return (self.on_diagnostics(&msg["params"]), Vec::new());
            }
            return (None, Vec::new());
        }
        let Some(id) = msg.get("id").and_then(Value::as_i64) else {
            return (None, Vec::new());
        };
        let Some((what, _)) = self.pending.remove(&id) else {
            return (None, Vec::new());
        };
        let result = &msg["result"];
        match what {
            Pending::Initialize => {
                if msg.get("error").is_some() {
                    self.state = State::Failed("texlab didn't start.".into());
                    return (None, Vec::new());
                }
                self.state = State::Ready;
                // 2 is Incremental, as a number or as `change` in the options object.
                let sync = &result["capabilities"]["textDocumentSync"];
                self.incremental = sync.as_u64().or_else(|| sync["change"].as_u64()) == Some(2);
                (
                    Some(Answer::Ready),
                    vec![Self::notify("initialized", json!({}))],
                )
            }
            Pending::Complete { path, head, from } => {
                let list = result.get("items").unwrap_or(result);
                let items = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|i| {
                                let label = i["label"].as_str()?.to_owned();
                                let detail = i["detail"].as_str().map(str::to_owned);
                                let documentation = match &i["documentation"] {
                                    Value::String(s) if !s.trim().is_empty() => {
                                        Some(s.trim().to_owned())
                                    }
                                    Value::Object(o) => {
                                        let raw = o
                                            .get("value")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default();
                                        let doc = if o.get("kind").and_then(Value::as_str)
                                            == Some("markdown")
                                        {
                                            plain(raw)
                                        } else {
                                            raw.trim().to_owned()
                                        };
                                        (!doc.is_empty()).then_some(doc)
                                    }
                                    _ => None,
                                };
                                let insert_text = i["textEdit"]["newText"]
                                    .as_str()
                                    .or_else(|| i["insertText"].as_str())
                                    .map(str::to_owned);
                                let filter_text = i["filterText"].as_str().map(str::to_owned);
                                let kind = i["kind"].as_u64();
                                Some(CompletionItem {
                                    label,
                                    detail,
                                    documentation,
                                    insert_text,
                                    filter_text,
                                    kind,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (
                    Some(Answer::Completion {
                        path,
                        head,
                        from,
                        items,
                    }),
                    Vec::new(),
                )
            }
            Pending::Hover {
                path,
                version,
                line,
                col,
            } => {
                let c = &result["contents"];
                let text = match c {
                    Value::String(s) => s.clone(),
                    Value::Object(o)
                        if o.get("kind").and_then(Value::as_str) == Some("markdown") =>
                    {
                        plain(o.get("value").and_then(Value::as_str).unwrap_or_default())
                    }
                    Value::Object(o) => o
                        .get("value")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    Value::Array(a) => a
                        .iter()
                        .filter_map(|x| {
                            x.as_str()
                                .map(str::to_owned)
                                .or_else(|| x["value"].as_str().map(str::to_owned))
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                    _ => String::new(),
                };
                let text = text.trim().to_owned();
                (
                    if text.is_empty() {
                        None
                    } else {
                        Some(Answer::Hover {
                            path,
                            version,
                            line,
                            col,
                            text,
                        })
                    },
                    Vec::new(),
                )
            }
            Pending::CodeActions {
                path,
                version,
                line,
            } => {
                let parse_edit = |e: &Value| -> Option<TextEdit> {
                    Some(TextEdit {
                        start: (
                            e["range"]["start"]["line"].as_u64()? as usize,
                            e["range"]["start"]["character"].as_u64()? as usize,
                        ),
                        end: (
                            e["range"]["end"]["line"].as_u64()? as usize,
                            e["range"]["end"]["character"].as_u64()? as usize,
                        ),
                        new_text: e["newText"].as_str()?.to_owned(),
                    })
                };
                let actions = result
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                let title = a["title"].as_str()?.to_owned();
                                let mut edits: Vec<(String, Vec<TextEdit>)> = Vec::new();
                                if let Some(changes) = a["edit"]["changes"].as_object() {
                                    for (uri, list) in changes {
                                        let rel = from_uri(&self.root, uri)?;
                                        let list: Vec<TextEdit> = list
                                            .as_array()?
                                            .iter()
                                            .filter_map(parse_edit)
                                            .collect();
                                        if !list.is_empty() {
                                            edits.push((rel, list));
                                        }
                                    }
                                } else if let Some(docs) = a["edit"]["documentChanges"].as_array() {
                                    for d in docs {
                                        let uri = d["textDocument"]["uri"].as_str()?;
                                        let rel = from_uri(&self.root, uri)?;
                                        let list: Vec<TextEdit> = d["edits"]
                                            .as_array()?
                                            .iter()
                                            .filter_map(parse_edit)
                                            .collect();
                                        if !list.is_empty() {
                                            edits.push((rel, list));
                                        }
                                    }
                                }
                                (!edits.is_empty()).then_some(CodeAction { title, edits })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (
                    Some(Answer::CodeActions {
                        path,
                        version,
                        line,
                        actions,
                    }),
                    Vec::new(),
                )
            }
            Pending::Definition | Pending::References => {
                let list: Vec<&Value> = match result {
                    Value::Array(a) => a.iter().collect(),
                    Value::Null => Vec::new(),
                    v => vec![v],
                };
                let places = list
                    .into_iter()
                    .filter_map(|l| {
                        let uri = l["uri"].as_str().or_else(|| l["targetUri"].as_str())?;
                        let range = if l.get("range").is_some() {
                            &l["range"]
                        } else {
                            &l["targetSelectionRange"]
                        };
                        let path = from_uri(&self.root, uri)?;
                        let line = range["start"]["line"].as_u64()? as usize;
                        let col = range["start"]["character"].as_u64().unwrap_or(0) as usize;
                        Some((path, line + 1, col))
                    })
                    .collect();
                (
                    Some(Answer::Locations {
                        references: what == Pending::References,
                        places,
                    }),
                    Vec::new(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // lsp.test.ts
    #[test]
    fn maps_paths_and_uris() {
        assert_eq!(
            to_uri("/home/a/p", "ch 1/x#.tex"),
            "file:///home/a/p/ch%201/x%23.tex"
        );
        assert_eq!(
            from_uri("/home/a/p", "file:///home/a/p/ch%201/x%23.tex").as_deref(),
            Some("ch 1/x#.tex")
        );
        assert_eq!(from_uri("/home/a/p", "file:///elsewhere/x.tex"), None);
        assert_eq!(to_uri("C:\\Users\\p", "a.tex"), "file:///C:/Users/p/a.tex");
        assert_eq!(
            from_uri("C:\\Users\\p", "file:///c:/Users/p/a.tex").as_deref(),
            Some("a.tex")
        );
        assert_eq!(utf16_col("a😀b", 2), 3);
    }

    #[test]
    fn opens_changes_and_closes_documents_once_ready() {
        let mut l = Lsp::new("/p");
        let init = l.initialize();
        assert!(init.contains("\"initialize\""));
        let text = || "x".to_string();
        assert!(
            l.sync(&[("main.tex".into(), 1, &text, None)]).is_empty(),
            "nothing before initialize"
        );
        let (a, out) = l.on_message(r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#);
        assert_eq!(a, Some(Answer::Ready));
        assert!(out[0].contains("initialized"));
        let msgs = l.sync(&[
            ("main.tex".into(), 1, &text, None),
            ("fig.png".into(), 1, &text, None),
        ]);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("didOpen"));
        assert!(l.sync(&[("main.tex".into(), 1, &text, None)]).is_empty());
        assert!(l.sync(&[("main.tex".into(), 2, &text, None)])[0].contains("didChange"));
        assert!(l.sync(&[])[0].contains("didClose"));
    }

    #[test]
    fn a_restarted_texlab_gets_the_open_files_again_and_late_problems_are_ignored() {
        let text = || "x".to_string();
        let mut l = Lsp::new("/p");
        l.initialize();
        l.on_message(r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#);
        assert!(l.sync(&[("main.tex".into(), 1, &text, None)])[0].contains("didOpen"));
        l.sync(&[("main.tex".into(), 2, &text, None)]);
        let late = r#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///p/main.tex","version":1,"diagnostics":[{"range":{"start":{"line":0}},"message":"old"}]}}"#;
        assert_eq!(
            l.on_message(late).0,
            None,
            "computed for the text before the edit"
        );
        // The new server starts: state is Starting, then Ready again.
        l.state = State::Starting;
        l.initialize();
        l.on_message(r#"{"jsonrpc":"2.0","id":2,"result":{"capabilities":{}}}"#);
        assert!(l.sync(&[("main.tex".into(), 2, &text, None)])[0].contains("didOpen"));
    }

    #[test]
    fn requests_texlab_never_answers_are_given_up_on() {
        let text = || "x".to_string();
        let mut l = Lsp::new("/p");
        l.initialize();
        l.on_message(r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#);
        l.sync(&[("main.tex".into(), 1, &text, None)]);
        l.definition("main.tex", 0, 0).unwrap();
        assert!(l.expire(std::time::Duration::from_secs(60)).is_empty());
        assert_eq!(
            l.expire(std::time::Duration::ZERO),
            vec![Pending::Definition]
        );
    }

    #[test]
    fn sends_only_the_changes_when_texlab_takes_them() {
        use crate::editor::buffer::LspEdit;
        let edit = || {
            Some(vec![LspEdit {
                start: (0, 1),
                end: (0, 1),
                text: "y".into(),
            }])
        };
        let text = || "xy".to_string();
        for (sync, ranges) in [("2", true), (r#"{"change":2}"#, true), ("1", false)] {
            let mut l = Lsp::new("/p");
            l.initialize();
            l.on_message(&format!(
                r#"{{"id":1,"result":{{"capabilities":{{"textDocumentSync":{sync}}}}}}}"#
            ));
            l.sync(&[("main.tex".into(), 1, &text, None)]);
            let msg = l.sync(&[("main.tex".into(), 2, &text, edit())]).remove(0);
            assert_eq!(msg.contains("\"range\""), ranges, "{sync}: {msg}");
            assert_eq!(msg.contains("\"xy\""), !ranges, "{sync}: {msg}");
            // Too many edits piled up: the whole text.
            let msg = l.sync(&[("main.tex".into(), 3, &text, None)]).remove(0);
            assert!(msg.contains("\"xy\""));
        }
    }

    #[test]
    fn keeps_texlab_s_problems_for_open_files() {
        let mut l = Lsp::new("/p");
        l.initialize();
        l.on_message(r#"{"id":1,"result":{}}"#);
        let text = || "x".to_string();
        l.sync(&[("main.tex".into(), 1, &text, None)]);
        let note = r#"{"method":"textDocument/publishDiagnostics","params":{"uri":"file:///p/main.tex","diagnostics":[
            {"range":{"start":{"line":4,"character":0},"end":{"line":4,"character":3}},"severity":1,"message":"Undefined reference"},
            {"range":{"start":{"line":5,"character":0},"end":{"line":5,"character":3}},"severity":4,"message":"Unused label"}]}}"#;
        let (a, _) = l.on_message(note);
        assert_eq!(a, Some(Answer::Diagnostics("main.tex".into())));
        assert_eq!(
            l.diagnostics["main.tex"],
            [Diagnostic {
                line: 5,
                start_col: Some(0),
                end_col: Some(3),
                severity: Severity::Error,
                message: "Undefined reference".to_string(),
            }]
        );
        // Not open: ignored. Closed: forgotten.
        let other = note.replace("main.tex", "other.tex");
        assert_eq!(l.on_message(&other).0, None);
        l.sync(&[]);
        assert!(l.diagnostics.is_empty());
    }

    #[test]
    fn diagnostics_are_sorted_and_deduplicated() {
        let mut l = Lsp::new("/p");
        l.initialize();
        l.on_message(r#"{"id":1,"result":{}}"#);
        let text = || "x".to_string();
        l.sync(&[("main.tex".into(), 1, &text, None)]);
        let (answer, _) = l.on_message(r#"{"method":"textDocument/publishDiagnostics","params":{"uri":"file:///p/main.tex","diagnostics":[
            {"range":{"start":{"line":4,"character":0}},"severity":1,"message":"same"},
            {"range":{"start":{"line":5,"character":0}},"severity":2,"message":"other"},
            {"range":{"start":{"line":4,"character":0}},"severity":1,"message":"same"}]}}"#);
        assert_eq!(answer, Some(Answer::Diagnostics("main.tex".into())));
        let list = &l.diagnostics["main.tex"];
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].message, "same");
        assert_eq!(list[1].message, "other");
    }

    #[test]
    fn markdown_hovers_read_as_plain_text() {
        assert_eq!(
            plain("D. Knuth: *The Art*. (1968)."),
            "D. Knuth: The Art. (1968)."
        );
        assert_eq!(
            plain("# Title\n`\\\\section` \\* x"),
            "Title\n\\section * x"
        );
        assert_eq!(plain("snake_case_name"), "snake_case_name");
    }

    #[test]
    fn reads_completions_and_locations() {
        let mut l = Lsp::new("/p");
        l.initialize();
        l.on_message(r#"{"id":1,"result":{}}"#);
        let text = || "\\sec".to_string();
        l.sync(&[("main.tex".into(), 1, &text, None)]);
        l.complete("main.tex", 0, 4, 4, 1).unwrap();
        let (a, _) = l.on_message(r#"{"id":2,"result":{"isIncomplete":true,"items":[{"label":"section","detail":"built-in"},{"label":"sectionmark"}]}}"#);
        let Some(Answer::Completion { items, from, .. }) = a else {
            panic!()
        };
        assert_eq!(from, 1);
        assert_eq!(items[0].label, "section");
        assert_eq!(items[0].detail.as_deref(), Some("built-in"));
        l.definition("main.tex", 0, 1).unwrap();
        let (a, _) = l.on_message(r#"{"id":3,"result":[{"uri":"file:///p/refs.bib","range":{"start":{"line":4,"character":2},"end":{"line":4,"character":5}}}]}"#);
        assert_eq!(
            a,
            Some(Answer::Locations {
                references: false,
                places: vec![("refs.bib".into(), 5, 2)]
            })
        );
        // A server request gets an answer.
        let (_, out) = l.on_message(
            r#"{"id":"x","method":"workspace/configuration","params":{"items":[{},{}]}}"#,
        );
        assert!(out[0].contains("[null,null]"));
    }

    #[test]
    fn from_uri_handles_case_expanding_unicode_without_panicking() {
        // P0-5: 'İ' and 'ẞ' expand in byte length when lowercased; must never panic.
        assert_eq!(
            from_uri(
                "/tmp/İ_ẞ_project",
                "file:///tmp/%C4%B0_%E1%BA%9E_project/main.tex"
            ),
            Some("main.tex".into())
        );
        assert_eq!(
            from_uri("/tmp/İ_ẞ_project", "file:///tmp/other/main.tex"),
            None
        );
    }
}
