// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod editor;
mod icons;
mod instance;
mod lsp;
mod panels;
mod pdf;
mod screens;
mod shortcuts;
mod spell;
mod store;
#[cfg(test)]
mod testing;
mod theme;
mod when;
mod widgets;
mod workspace;
mod writing;

fn main() -> eframe::Result {
    // A second launch hands its file or link to the running Lemmata and quits. Not in debug
    // builds, so a test build can run next to an installed Lemmata.
    #[cfg(not(debug_assertions))]
    {
        let args: Vec<String> = std::env::args().collect();
        let cwd = std::env::current_dir().unwrap_or_default();
        if instance::forward(&store::Dirs::system().cache, &args, &cwd) {
            return Ok(());
        }
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Lemmata")
            .with_app_id("dev.briggen.lemmata")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Lemmata",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
