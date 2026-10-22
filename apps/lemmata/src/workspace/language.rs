//! texlab for the open project: starting it, keeping it told about the open files, and acting
//! on its answers. The protocol itself is `crate::lsp`.

use super::{Tone, Workspace};
use crate::backend::{Backend, Msg};
use std::collections::HashMap;

impl Workspace {
    pub(super) fn lsp_send(&self, _backend: &Backend, messages: Vec<String>) {
        if let Some(tx) = &self.lsp_tx {
            for body in messages {
                let _ = tx.send(body);
            }
        }
    }

    /// Ends texlab with the project.
    pub fn stop_lsp(&self, backend: &Backend) {
        if let Some(generation) = self.lsp.generation {
            backend.spawn_async(
                move |shell| async move { Msg::Done(shell.lsp_stop(generation).await) },
            );
        }
    }

    /// Tells texlab about the open files' current text.
    pub(super) fn lsp_sync(&mut self, backend: &Backend) {
        if self.lsp.state != crate::lsp::State::Ready {
            return;
        }
        let mut edits: HashMap<String, _> = self
            .editors
            .iter_mut()
            .map(|(p, e)| (p.clone(), e.doc.take_lsp_edits()))
            .collect();
        #[allow(clippy::type_complexity)]
        let texts: Vec<(String, u64, Box<dyn Fn() -> String + '_>)> = self
            .editors
            .iter()
            .map(|(p, e)| {
                (
                    p.clone(),
                    e.doc.version,
                    Box::new(move || e.doc.text()) as Box<dyn Fn() -> String>,
                )
            })
            .collect();
        let docs: Vec<crate::lsp::SyncDoc> = texts
            .iter()
            .map(|(p, v, f)| (p.clone(), *v, f.as_ref(), edits.remove(p).flatten()))
            .collect();
        let msgs = self.lsp.sync(&docs);
        drop(docs);
        drop(texts);
        self.lsp_send(backend, msgs);
    }

    pub(super) fn on_lsp(&mut self, backend: &Backend, a: crate::lsp::Answer) {
        use crate::lsp::Answer;
        match a {
            Answer::Ready => self.lsp_sync(backend),
            Answer::Completion {
                path,
                head,
                from,
                items,
            } => {
                self.lookups.server = Some((path, head, from, items));
                self.lookups.generation += 1;
            }
            Answer::Hover {
                path,
                line,
                col,
                text,
            } => {
                self.lookups.hover = Some((path, line, col, text));
            }
            // Editors take the new list with the build's (`problems_of`).
            Answer::Diagnostics(_) => self.build.diag_gen += 1,
            Answer::Locations {
                references: false,
                places,
            } => {
                if let Some((path, line, _)) = places.into_iter().next() {
                    self.goto(backend, &path, line);
                }
            }
            Answer::Locations {
                references: true,
                places,
            } => {
                let rows = places
                    .into_iter()
                    .map(|(path, line, col)| {
                        let text = match self.editors.get(&path) {
                            Some(e) if line >= 1 && line <= e.doc.line_count() => {
                                e.doc.line(line - 1)
                            }
                            _ => std::fs::read_to_string(self.info.root.join(&path))
                                .ok()
                                .and_then(|t| {
                                    t.lines().nth(line.saturating_sub(1)).map(str::to_owned)
                                })
                                .unwrap_or_default(),
                        };
                        (path, line, col, text.trim().to_owned())
                    })
                    .collect();
                self.references = Some(rows);
            }
        }
    }

    /// texlab's messages: it started or stopped, or it says something.
    pub(super) fn on_lsp_msg(&mut self, backend: &Backend, msg: Msg) {
        match msg {
            Msg::LspStarted(Ok(generation)) => {
                self.lsp.generation = Some(generation);
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                self.lsp_tx = Some(tx);
                backend.spawn_async(move |shell| async move {
                    while let Some(body) = rx.recv().await {
                        if shell.lsp_send(generation, body).await.is_err() {
                            return Msg::LspLost(generation);
                        }
                    }
                    Msg::Done(Ok(()))
                });
                let init = self.lsp.initialize();
                self.lsp_send(backend, vec![init]);
            }
            Msg::LspStarted(Err(e)) => self.lsp.state = crate::lsp::State::Failed(e),
            Msg::LspLost(generation) => {
                if Some(generation) == self.lsp.generation
                    && self.lsp.state == crate::lsp::State::Ready
                {
                    self.lsp.state = crate::lsp::State::Failed("texlab stopped.".into());
                    self.notice(
                        "The language server (texlab) stopped answering, so completion and hover may be missing. Reopen the project to restart it.",
                        Tone::Info,
                    );
                }
            }
            Msg::Event(lemmata_shell::Event::LspMessage { generation, body }) => {
                if Some(generation) == self.lsp.generation {
                    let (answer, replies) = self.lsp.on_message(&body);
                    self.lsp_send(backend, replies);
                    if let Some(a) = answer {
                        self.on_lsp(backend, a);
                    }
                }
            }
            Msg::Event(lemmata_shell::Event::LspExit { generation })
                if Some(generation) == self.lsp.generation =>
            {
                self.lsp.state = crate::lsp::State::Failed("texlab stopped.".into());
            }
            _ => {}
        }
    }
}
