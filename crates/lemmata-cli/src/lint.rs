//! `lemmata lint`: builds the project, then adds checks that report each
//! problem with its file and line: citation keys in no .bib file, and
//! references to labels that are never defined.

use crate::{compile, open, print_diagnostics};
use lemmata_log::{Diagnostic, Kind, Severity};
use lemmata_project::Project;
use lemmata_project::symbols::{self, Kind as Sym, Role};
use lemmata_tex::Status;
use std::collections::HashSet;
use std::path::Path;

/// Prints the report and returns whether the check passed.
pub async fn run(dir: &Path, json: bool, strict: bool) -> anyhow::Result<bool> {
    let project = open(dir)?;
    let outcome = compile(&project).await?;

    let checks = static_checks(&project);
    // A build's own "undefined" warning is replaced by the located one from
    // the check, so each problem is printed once.
    let mut diagnostics: Vec<Diagnostic> = outcome
        .diagnostics
        .into_iter()
        .filter(|d| !checks.iter().any(|c| c.kind == d.kind))
        .collect();
    diagnostics.extend(checks);

    let errors = count(&diagnostics, Severity::Error);
    let warnings = count(&diagnostics, Severity::Warning);
    if json {
        println!("{}", serde_json::to_string_pretty(&diagnostics)?);
    } else {
        print_diagnostics(&diagnostics);
        if diagnostics.is_empty() {
            println!("No problems found.");
        } else {
            println!("{errors} error(s), {warnings} warning(s)");
        }
    }

    let failed = outcome.status != Status::Success || errors > 0 || (strict && warnings > 0);
    Ok(!failed)
}

fn count(diagnostics: &[Diagnostic], severity: Severity) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.severity == severity)
        .count()
}

/// Citation keys that no .bib file defines, and labels that are referenced
/// but never defined. Each is reported once, at its first use.
/// shortcut: labels from another document (`\externaldocument`) count as undefined.
fn static_checks(project: &Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    let missing: HashSet<String> = crate::bib::missing_keys(project).into_iter().collect();

    let occurrences = symbols::scan(project);
    let defined_labels: HashSet<&str> = occurrences
        .iter()
        .filter(|o| o.kind == Sym::Label && o.role == Role::Def)
        .map(|o| o.name.as_str())
        .collect();

    let mut seen: HashSet<(bool, String)> = HashSet::new();
    for o in occurrences.iter().filter(|o| o.role == Role::Use) {
        if !seen.insert((o.kind == Sym::Cite, o.name.clone())) {
            continue;
        }
        let (kind, message, hint) = match o.kind {
            Sym::Cite if missing.contains(&o.name) => (
                Kind::UndefinedCitation {
                    key: o.name.clone(),
                },
                format!("citation `{}` is in no .bib file", o.name),
                "Check the spelling, or add the entry to a .bib file.",
            ),
            Sym::Label if !defined_labels.contains(o.name.as_str()) => (
                Kind::UndefinedReference {
                    key: o.name.clone(),
                },
                format!("reference to label `{}`, which is never defined", o.name),
                "Check the spelling, or add a \\label with this name.",
            ),
            _ => continue,
        };
        out.push(Diagnostic {
            severity: Severity::Error,
            kind,
            file: Some(o.file.clone()),
            line: Some(o.line),
            message,
            hint: Some(hint.into()),
            fix: None,
        });
    }
    out
}
