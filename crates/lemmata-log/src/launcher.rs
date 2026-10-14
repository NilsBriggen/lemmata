//! Explains a build that stopped before TeX wrote a log: latexmk could not
//! start, or its wrapper refused to run. Its own output is all there is.

use crate::{Diagnostic, Fix, Kind, Severity};

const PERL_COMMAND: &str = "winget install StrawberryPerl.StrawberryPerl";

/// `output` is the tail of latexmk's stdout and stderr.
pub fn launcher_failure(output: &str) -> Diagnostic {
    let text = output.trim();
    let lower = text.to_ascii_lowercase();
    let (message, hint, fix) = if mentions_perl(&lower) {
        (
            "latexmk needs Perl, and Perl was not found.".to_string(),
            Some("MiKTeX does not include Perl. Install it, then restart Lemmata so it picks up the new PATH.".to_string()),
            Some(Fix {
                explanation: "Install Perl (latexmk is a Perl script).".into(),
                command: Some(PERL_COMMAND.into()),
                package: None,
            }),
        )
    } else if mentions_app_control(&lower) {
        (
            "Windows blocked a program that latexmk needs.".to_string(),
            Some("An application control policy (Smart App Control, AppLocker or similar) refuses to run it, often Perl. Lemmata then builds with pdflatex directly, without latexmk. To use latexmk, allow the blocked program in Windows Security, or use a Perl that is signed.".to_string()),
            None,
        )
    } else if lower.contains("miktex")
        && (lower.contains("install missing")
            || lower.contains("auto-install")
            || lower.contains("on-the-fly"))
    {
        (
            "MiKTeX wants to install a missing package but cannot ask.".to_string(),
            Some("Open MiKTeX Console, Settings, and set \"Install missing packages on-the-fly\" to Always. Then compile again.".to_string()),
            Some(Fix {
                explanation: "Turn on unattended package installation.".into(),
                command: Some("initexmf --set-config-value [MPM]AutoInstall=1".into()),
                package: None,
            }),
        )
    } else if mentions_miktex_updates(&lower) {
        (
            "MiKTeX refuses to run until it has checked for updates.".to_string(),
            Some("MiKTeX stops its tools until its package database has been refreshed. Open MiKTeX Console, choose Updates, and check for updates (installing them is optional). Then compile again.".to_string()),
            Some(Fix {
                explanation: "Refresh MiKTeX's package database from a terminal (the same as Check for updates in MiKTeX Console).".into(),
                command: Some("miktex packages update-package-database".into()),
                package: None,
            }),
        )
    } else if text.is_empty() {
        (
            "latexmk stopped before TeX produced a log, and printed nothing.".to_string(),
            Some("Run latexmk in a terminal in the project folder to see why.".to_string()),
            None,
        )
    } else {
        (
            format!("latexmk stopped before TeX produced a log:\n{text}"),
            None,
            None,
        )
    };
    Diagnostic {
        severity: Severity::Error,
        kind: Kind::Other,
        file: None,
        line: None,
        message,
        hint,
        fix,
    }
}

/// Windows error 4551: "an application control policy has blocked this file",
/// in the user's language (the number is the same in all of them).
fn mentions_app_control(lower: &str) -> bool {
    lower.contains("application control")
        || lower.contains("anwendungssteuerung")
        || lower.contains("error 4551")
        || lower.contains("result: 4551")
}

/// MiKTeX's "you have not checked for updates" stop, in the language of the
/// user's Windows ("MiKTeX-Updates" in German, "mises à jour" in French, ...).
/// The marker "major issue" next to MiKTeX stays English in every language.
fn mentions_miktex_updates(lower: &str) -> bool {
    lower.contains("miktex")
        && (lower.contains("major issue")
            || lower.contains("update required")
            || lower.contains("updates")
            || lower.contains("mises à jour"))
}

fn mentions_perl(lower: &str) -> bool {
    lower.contains("perl")
        && (lower.contains("not found")
            || lower.contains("not recognized")
            || lower.contains("nicht gefunden")
            || lower.contains("introuvable")
            || lower.contains("cannot find")
            || lower.contains("can't find")
            || lower.contains("script engine")
            || lower.contains("no such file")
            || lower.contains("not installed"))
        || lower.contains("script engine")
        || lower.contains("can't locate")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_perl_gets_the_winget_command() {
        let d = launcher_failure("MiKTeX: The perl script engine could not be found.\n");
        assert!(d.message.contains("Perl"));
        assert_eq!(d.fix.unwrap().command.as_deref(), Some(PERL_COMMAND));
    }

    #[test]
    fn perl_module_errors_count() {
        let d = launcher_failure("Can't locate Foo.pm in @INC");
        assert!(d.message.contains("Perl"));
    }

    #[test]
    fn miktex_update_prompt_points_at_the_console() {
        let d = launcher_failure("major issue: So far, you have not checked for MiKTeX updates.");
        assert!(d.hint.unwrap().contains("MiKTeX Console"));
    }

    #[test]
    fn the_german_update_prompt_from_the_bug_report_is_recognised() {
        let text = "Entschuldigung, aber latexmk.exe schlug fehl.\n\nlatexmk: major issue: Bis jetzt haben Sie noch nicht nach MiKTeX-Updates gesucht.";
        let d = launcher_failure(text);
        assert!(d.message.contains("checked for updates"));
        assert_eq!(
            d.fix.unwrap().command.as_deref(),
            Some("miktex packages update-package-database")
        );
    }

    #[test]
    fn a_blocked_perl_is_not_blamed_on_missing_updates() {
        // MiKTeX prints the update line at the end of every run, whatever went wrong.
        let text = "FATAL latexmk - Windows API error 4551: Eine Anwendungssteuerungsrichtlinie hat diese Datei blockiert.\nERROR latexmk - major issue: Bis jetzt haben Sie noch nicht nach MiKTeX-Updates gesucht.";
        let d = launcher_failure(text);
        assert!(d.message.contains("blocked"), "{}", d.message);
        assert!(d.fix.is_none());
        // On its own, the update line still gets its own explanation.
        assert!(launcher_failure("ERROR latexmk - major issue: Bis jetzt haben Sie noch nicht nach MiKTeX-Updates gesucht.").message.contains("checked for updates"));
    }

    #[test]
    fn auto_install_prompt() {
        let d = launcher_failure("MiKTeX cannot install missing packages on-the-fly here");
        assert!(d.fix.unwrap().command.unwrap().contains("AutoInstall"));
    }

    #[test]
    fn unknown_output_is_shown_and_empty_output_is_explained() {
        assert!(launcher_failure("boom").message.contains("boom"));
        assert!(launcher_failure("  ").message.contains("printed nothing"));
    }
}
