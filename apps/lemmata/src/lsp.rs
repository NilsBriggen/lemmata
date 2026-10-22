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
        line: usize,
        col: usize,
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
        items: Vec<(String, Option<String>)>,
    },
    Hover {
        path: String,
        line: usize,
        col: usize,
        text: String,
    },
    /// Places in the project: path relative to the root, 1-based line, and 0-based column (chars).
    Locations {
        references: bool,
        places: Vec<(String, usize, usize)>,
    },
    /// texlab's problems for this file changed (see `Lsp::diagnostics`).
    Diagnostics(String),
}

/// How serious texlab finds a problem (LSP's 1 to 3; hints such as "Unused label" are left out).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

pub struct Lsp {
    pub state: State,
    pub generation: Option<u64>,
    next_id: i64,
    pending: HashMap<i64, Pending>,
    /// Open documents and the editor version texlab last saw.
    opened: HashMap<String, (i64, u64)>,
    root: String,
    /// texlab takes changed ranges rather than the whole text (its `textDocumentSync`).
    incremental: bool,
    /// Problems texlab sees as the text is typed, without a build: unclosed braces, undefined
    /// references and citations, duplicate labels. Per file: 1-based line, severity, message.
    pub diagnostics: HashMap<String, Vec<(usize, Severity, String)>>,
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
    norm.to_lowercase()
        .starts_with(&base.to_lowercase())
        .then(|| norm[base.len()..].to_owned())
}

/// LSP columns count UTF-16 units; the editor counts chars.
pub fn utf16_col(line: &str, chars: usize) -> usize {
    line.chars().take(chars).map(char::len_utf16).sum()
}

fn language(path: &str) -> &'static str {
    if path.ends_with(".bib") {
        "bibtex"
    } else {
        "latex"
    }
}

/// texlab's Markdown as the plain text the hover card shows: no emphasis, code or heading
/// marks, and no backslash escapes. shortcut: no links or lists, which texlab's hovers don't use.
fn plain(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for line in md.lines() {
        let line = line.trim_start_matches('#').trim_start();
        if line.starts_with("```") {
            continue;
        }
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if chars.peek().is_some_and(char::is_ascii_punctuation) => {
                    out.extend(chars.next())
                }
                '*' | '`' => {}
                '_' if out.ends_with([' ', '_'])
                    || out.is_empty()
                    || chars.peek() == Some(&'_') => {}
                _ => out.push(c),
            }
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
        self.pending.insert(id, what);
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
    }

    fn notify(method: &str, params: Value) -> String {
        json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
    }

    /// The first message, once the process is up.
    pub fn initialize(&mut self) -> String {
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
                        "completion": { "completionItem": { "snippetSupport": false } },
                        "hover": { "contentFormat": ["plaintext", "markdown"] },
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

    pub fn hover(&mut self, path: &str, line: usize, utf16: usize, col: usize) -> Option<String> {
        (self.state == State::Ready && self.opened.contains_key(path)).then(|| {
            let p = self.position(path, line, utf16);
            self.request(
                "textDocument/hover",
                p,
                Pending::Hover {
                    path: path.into(),
                    line,
                    col,
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
        if !self.opened.contains_key(&path) {
            return None;
        }
        let list = params["diagnostics"]
            .as_array()?
            .iter()
            .filter_map(|d| {
                let severity = match d["severity"].as_u64().unwrap_or(1) {
                    1 => Severity::Error,
                    2 => Severity::Warning,
                    3 => Severity::Info,
                    _ => return None,
                };
                let line = d["range"]["start"]["line"].as_u64()? as usize + 1;
                Some((line, severity, d["message"].as_str()?.to_owned()))
            })
            .collect::<Vec<_>>();
        let mut list = list;
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
        let Some(what) = self.pending.remove(&id) else {
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
                                Some((label, detail))
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
            Pending::Hover { path, line, col } => {
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
                            line,
                            col,
                            text,
                        })
                    },
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
            [(5, Severity::Error, "Undefined reference".to_string())]
        );
        // Not open: ignored. Closed: forgotten.
        let other = note.replace("main.tex", "other.tex");
        assert_eq!(l.on_message(&other).0, None);
        l.sync(&[]);
        assert!(l.diagnostics.is_empty());
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
        assert_eq!(items[0], ("section".into(), Some("built-in".into())));
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
}
