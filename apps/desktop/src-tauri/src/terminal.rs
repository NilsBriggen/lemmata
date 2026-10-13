//! Opens the user's terminal with an install command waiting for
//! confirmation. Nothing runs until the user presses Enter there, and sudo
//! asks for the password in the terminal, never in Lemmata.

use std::process::Command;

/// Shown in the terminal before the command runs. `$1` is the command.
#[cfg(unix)]
const CONFIRM_SCRIPT: &str = r#"printf '\nLemmata wants to run:\n\n    %s\n\nPress Enter to run it, or close this window to cancel. ' "$1"; read -r _; eval "$1"; status=$?; printf '\nFinished (exit code %s). Go back to Lemmata and click Check again or Recompile.\nPress Enter to close. ' "$status"; read -r _"#;

/// Terminals to try on Linux, with the flag that runs a command.
#[cfg(all(unix, not(target_os = "macos")))]
const LINUX_TERMINALS: &[(&str, &[&str])] = &[
    ("x-terminal-emulator", &["-e"]),
    ("kgx", &["--"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    ("kitty", &[]),
    ("alacritty", &["-e"]),
    ("foot", &[]),
    ("ghostty", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("xfce4-terminal", &["-x"]),
    ("xterm", &["-e"]),
];

pub fn open_with_command(command: &str) -> Result<(), String> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut candidates: Vec<(String, Vec<&str>)> = Vec::new();
        if let Ok(t) = std::env::var("TERMINAL") {
            candidates.push((t, vec!["-e"]));
        }
        candidates.extend(
            LINUX_TERMINALS
                .iter()
                .map(|(t, a)| (t.to_string(), a.to_vec())),
        );
        for (term, flags) in candidates {
            let Ok(path) = which::which(&term) else {
                continue;
            };
            let spawned = Command::new(path)
                .args(flags)
                .args(["bash", "-c", CONFIRM_SCRIPT, "lemmata", command])
                .spawn();
            if spawned.is_ok() {
                return Ok(());
            }
        }
        Err("No terminal app was found. Copy the command and run it in your terminal.".into())
    }
    #[cfg(target_os = "macos")]
    {
        // Terminal.app runs `do script` text in a new window.
        let quoted = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
        let shell = format!(
            "bash -c {} lemmata {}",
            quoted(CONFIRM_SCRIPT),
            quoted(command)
        );
        let apple = format!(
            "tell application \"Terminal\"\nactivate\ndo script \"{}\"\nend tell",
            shell.replace('\\', "\\\\").replace('"', "\\\"")
        );
        Command::new("osascript")
            .args(["-e", &apple])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open Terminal: {e}"))
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `start` opens a new console; `pause` waits for confirmation.
        let line = format!(
            "echo Lemmata wants to run: & echo. & echo     {command} & echo. & echo Press any key to run it, or close this window to cancel. & pause >nul & {command} & echo. & pause"
        );
        Command::new("cmd")
            .raw_arg(format!("/c start \"Lemmata\" cmd /k \"{line}\""))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open a terminal: {e}"))
    }
}
