//! Spelling and grammar for the open project: which language, loading its dictionary (or
//! offering to download it), and sending paragraphs to LanguageTool.

use super::{Notice, NoticeAction, Tone, Workspace, tree};
use crate::backend::{Backend, Msg};
use crate::store::Settings;

impl Workspace {
    /// The spell-check language: a `% !TeX spellcheck` comment in the open file, the project's
    /// choice, or the app's default (guessed from babel or polyglossia for "auto").
    pub(super) fn spell_language(&self, settings: &Settings) -> String {
        let magic = self
            .active
            .as_ref()
            .filter(|a| tree::is_tex(a))
            .and_then(|a| self.editors.get(a))
            .and_then(|e| {
                let head: Vec<String> = (0..e.doc.line_count().min(30))
                    .map(|n| e.doc.line(n))
                    .collect();
                crate::writing::spell::magic_language(&head.join("\n"))
            });
        let fallback = if settings.spell_default == "auto" {
            self.detected.to_owned()
        } else {
            settings.spell_default.clone()
        };
        magic
            .or_else(|| self.info.config.spellcheck.clone())
            .unwrap_or(fallback)
    }

    /// Loads another spell-check language when the file, project or settings ask for one, and
    /// points grammar checking at the same language.
    pub(super) fn apply_language(&mut self, backend: &Backend, settings: &Settings) {
        let lang = self.spell_language(settings);
        let lang = if crate::writing::spell::LANGUAGES
            .iter()
            .any(|(c, _)| *c == lang)
        {
            lang
        } else {
            "off".to_owned()
        };
        let words = self.info.config.words.clone();
        if self.spell_asked.as_ref() != Some(&(lang.clone(), words.clone())) {
            self.spell_asked = Some((lang.clone(), words.clone()));
            let l = lang.clone();
            backend.spawn(move |shell| {
                let r = shell.spell_set_language(l.clone(), words);
                Msg::SpellLoaded((l != "off").then_some(l), r)
            });
            // A language whose dictionary isn't here yet is offered, once a session.
            if lang != "off" && self.dictionaries_offered.insert(lang.clone()) {
                backend.spawn(move |shell| match shell.spell_available() {
                    Ok(list) => match list.into_iter().find(|d| d.code == lang && !d.installed) {
                        Some(d) => Msg::Dictionary(d),
                        None => Msg::Done(Ok(())),
                    },
                    Err(_) => Msg::Done(Ok(())),
                });
            }
        }
        let lang = self
            .spell_asked
            .as_ref()
            .map_or("off".to_owned(), |a| a.0.clone());
        self.checks
            .grammar
            .configure(&settings.grammar, &settings.grammar_url, &lang);
        self.checks
            .grammar
            .set_ignored(&self.info.config.grammar_ignore);
    }

    /// Sends waiting paragraphs to LanguageTool, two at a time.
    pub(super) fn pump_grammar(&mut self, backend: &Backend) {
        let Some(target) = self.checks.grammar.target.clone() else {
            self.grammar_queue.clear();
            return;
        };
        while self.grammar_out < 2 && !self.grammar_queue.is_empty() {
            let text = self.grammar_queue.remove(0);
            self.grammar_out += 1;
            let t = target.clone();
            backend.spawn_async(move |shell| async move {
                let r = shell
                    .grammar_check(t.0.clone(), t.1.clone(), text.clone())
                    .await;
                Msg::GrammarFound(t, text, r)
            });
        }
    }

    /// A dictionary loaded or downloaded, one is missing, or LanguageTool answered.
    pub(super) fn on_check_msg(&mut self, backend: &Backend, msg: Msg) {
        match msg {
            Msg::SpellLoaded(lang, Ok(())) => {
                if self
                    .spell_asked
                    .as_ref()
                    .map(|a| (a.0 != "off").then(|| a.0.clone()))
                    == Some(lang.clone())
                {
                    self.checks.spell.set_loaded(lang);
                }
            }
            Msg::SpellLoaded(_, Err(_)) => {
                // Allow a retry; until then no word is marked.
                self.spell_asked = None;
                self.checks.spell.set_loaded(None);
            }
            Msg::Dictionary(d) => {
                let size = (d.size_kb / 1024).max(1);
                self.notices.push(
                    Notice::new(
                        format!(
                            "Spell check in {} needs its dictionary (downloads {size} MB).",
                            d.label
                        ),
                        Tone::Info,
                    )
                    .action(NoticeAction::Download(d.code)),
                );
            }
            Msg::SpellDownloaded(code, Ok(())) => {
                self.spell_asked = None;
                self.notice(
                    format!(
                        "Spell check in {} is on.",
                        crate::writing::spell::label(&code)
                    ),
                    Tone::Ok,
                );
            }
            Msg::SpellDownloaded(_, Err(e)) => self.notice(e, Tone::Error),
            Msg::GrammarFound(target, text, result) => {
                self.grammar_out = self.grammar_out.saturating_sub(1);
                if self.checks.grammar.target.as_ref() == Some(&target) {
                    match result {
                        Ok(m) => self.checks.grammar.store(text, m),
                        Err(e) => {
                            // Stops at the first failure; a later edit retries.
                            self.checks.grammar.asked.remove(&text);
                            self.checks.grammar.error = Some(e);
                            self.grammar_queue.clear();
                        }
                    }
                }
                self.pump_grammar(backend);
            }
            _ => {}
        }
    }
}
