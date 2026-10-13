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
    } else if lower.contains("miktex update required")
        || lower.contains("you have not checked for miktex updates")
    {
        (
            "MiKTeX asks for an update before it will run.".to_string(),
            Some(
                "Open MiKTeX Console, choose Updates, and install them. Then compile again."
                    .to_string(),
            ),
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

fn mentions_perl(lower: &str) -> bool {
    lower.contains("perl")
        && (lower.contains("not found")
            || lower.contains("not recognized")
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
