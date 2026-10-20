// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
#[allow(dead_code)]
mod screens;
#[allow(dead_code)]
mod spell;
#[allow(dead_code)]
mod store;
#[cfg(test)]
mod testing;
// shortcut: icons, widgets and some tokens are unused until the panels land in M2; drop the allows then.
#[allow(dead_code)]
mod icons;
#[allow(dead_code)]
mod theme;
#[allow(dead_code)]
mod widgets;

fn main() -> eframe::Result {
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
