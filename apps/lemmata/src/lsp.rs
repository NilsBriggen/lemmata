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
}

pub struct Lsp {
    pub state: State,
    pub generation: Option<u64>,
    next_id: i64,
    pending: HashMap<i64, Pending>,
    /// Open documents and the editor version texlab last saw.
    opened: HashMap<String, (i64, u64)>,
    root: String,
}

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
    pub fn sync(&mut self, docs: &[(String, u64, &dyn Fn() -> String)]) -> Vec<String> {
        if self.state != State::Ready {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (path, version, text) in docs {
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
                    out.push(Self::notify(
                        "textDocument/didChange",
                        json!({ "textDocument": { "uri": uri, "version": *v }, "contentChanges": [{ "text": text() }] }),
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
            l.sync(&[("main.tex".into(), 1, &text)]).is_empty(),
            "nothing before initialize"
        );
        let (a, out) = l.on_message(r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#);
        assert_eq!(a, Some(Answer::Ready));
        assert!(out[0].contains("initialized"));
        let msgs = l.sync(&[("main.tex".into(), 1, &text), ("fig.png".into(), 1, &text)]);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("didOpen"));
        assert!(l.sync(&[("main.tex".into(), 1, &text)]).is_empty());
        assert!(l.sync(&[("main.tex".into(), 2, &text)])[0].contains("didChange"));
        assert!(l.sync(&[])[0].contains("didClose"));
    }

    #[test]
    fn reads_completions_and_locations() {
        let mut l = Lsp::new("/p");
        l.initialize();
        l.on_message(r#"{"id":1,"result":{}}"#);
        let text = || "\\sec".to_string();
        l.sync(&[("main.tex".into(), 1, &text)]);
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
