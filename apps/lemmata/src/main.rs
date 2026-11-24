// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod editor;
mod icons;
mod instance;
mod lsp;
#[cfg(target_os = "macos")]
mod macos;
mod migrate;
mod panels;
mod pdf;
mod screens;
mod scroll;
mod shortcuts;
mod spell;
mod store;
#[cfg(test)]
mod testing;
mod theme;
#[cfg(target_os = "linux")]
mod wayland;
mod when;
mod widgets;
mod workspace;
mod writing;

fn main() -> eframe::Result {
    // Started as texlab's stand-in latexmk for a restricted project (lemmata_shell::lsp::latexmk_stub).
    #[cfg(windows)]
    if std::env::current_exe()
        .ok()
        .and_then(|e| e.file_stem().map(|s| s.eq_ignore_ascii_case("latexmk")))
        .unwrap_or(false)
    {
        std::process::exit(1);
    }
    // A second launch hands its file or link to the running Lemmata and quits. Not in debug
    // builds, so a test build can run next to an installed Lemmata.
    #[cfg(not(debug_assertions))]
    {
        let args: Vec<String> = std::env::args_os()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let cwd = std::env::current_dir().unwrap_or_default();
        if instance::forward(&store::Dirs::system().cache, &args, &cwd) {
            return Ok(());
        }
    }
    #[cfg(target_os = "macos")]
    macos::listen();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Lemmata")
            .with_app_id("dev.briggen.lemmata")
            // Without it eframe shows its own logo, and Windows the generic one until then.
            .with_icon(std::sync::Arc::new(app_icon()))
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Lemmata",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    );
    instance::release(&store::Dirs::system().cache);
    // An installed update: the new version (or its installer) starts once this one is gone.
    if let Some((program, args)) = app::AFTER_EXIT.lock().ok().and_then(|mut a| a.take()) {
        let _ = std::process::Command::new(program).args(args).spawn();
    }
    result
}

/// The window and taskbar icon.
fn app_icon() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../icons/icon.png"))
        .expect("the bundled icon is a PNG")
}

#[cfg(test)]
#[test]
fn the_app_icon_loads() {
    let icon = app_icon();
    assert_eq!((icon.width, icon.height), (512, 512));
}
