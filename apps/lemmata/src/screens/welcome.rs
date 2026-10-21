//! First run (Welcome.svelte): confirm TeX works, a few optional choices, then a way in.
//! Nothing is created unless the user asks; "Skip" is always there.

use crate::icons;
use crate::spell::LANGUAGES;
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Rich, Size};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
};
use lemmata_tex::detect::{Detection, Distribution};

#[derive(Debug, PartialEq)]
pub enum Action {
    /// "Skip" or "Go to my projects".
    Done,
    Example,
    NewProject,
    OpenFolder,
    Import,
    /// A setting changed; save it.
    Settings,
}

const EXTRAS: [(&str, &str); 4] = [
    ("synctex", "Jump between PDF and source"),
    ("biber", "biblatex bibliographies"),
    ("xelatex", "XeLaTeX, for system fonts"),
    ("lualatex", "LuaLaTeX"),
];

fn found(d: &Detection, name: &str) -> bool {
    d.tools.iter().any(|t| t.name == name && t.path.is_some())
}

pub fn show(
    ui: &mut Ui,
    p: &Palette,
    d: &Detection,
    step: &mut usize,
    settings: &mut Settings,
) -> Option<Action> {
    let mut action = None;
    // .welcome: a glow of accent at the top on --app, content in a 720 px column.
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, p.app);
    widgets::radial_glow(
        ui.painter(),
        pos2(full.center().x, full.top() - 0.1 * full.height()),
        Vec2::new(1200.0, 500.0),
        Palette::mix(p.accent, 12.0),
        0.7,
    );
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(28.0);
        widgets::column(ui, 720.0, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                steps(ui, p, *step);
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), 30.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if Button::new("Skip")
                            .kind(Kind::Ghost)
                            .ink(p.muted)
                            .show(ui, p)
                            .clicked()
                        {
                            action = Some(Action::Done);
                        }
                    },
                );
            });
            ui.add_space(0.08 * full.height());
            widgets::card(
                ui,
                p,
                14,
                Margin {
                    left: 40,
                    right: 40,
                    top: 36,
                    bottom: 28,
                },
                |ui| {
                    ui.spacing_mut().item_spacing.y = 14.0;
                    match *step {
                        0 => step_welcome(ui, p, d, step),
                        1 => step_settings(ui, p, step, settings, &mut action),
                        _ => step_start(ui, p, step, &mut action),
                    }
                },
            );
            ui.add_space(40.0);
        });
    });
    action
}

/// The numbered steps: current in accent, done in green with a check.
fn steps(ui: &mut Ui, p: &Palette, step: usize) {
    ui.spacing_mut().item_spacing.x = 22.0;
    for (i, label) in ["Welcome", "Setup", "Start"].into_iter().enumerate() {
        let color = if i == step {
            p.accent
        } else if i < step {
            p.ok
        } else {
            p.faint
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
            ui.painter()
                .circle_stroke(rect.center(), 10.25, Stroke::new(1.5, color));
            if i < step {
                icons::paint(ui.painter(), rect.shrink(5.0), icons::CHECK, color);
            } else {
                widgets::centered_text(
                    ui,
                    rect,
                    &(i + 1).to_string(),
                    FontId::new(11.0, theme::font::sans_600()),
                    color,
                );
            }
            ui.label(
                egui::RichText::new(label)
                    .font(FontId::new(13.0, theme::font::sans_500()))
                    .color(color),
            );
        });
    }
}

fn heading(ui: &mut Ui, p: &Palette, title: &str, lede: &str) {
    // Young Serif's "normal" line height is 1.378 em.
    ui.label(
        egui::RichText::new(title)
            .font(FontId::new(32.0, theme::font::display()))
            .color(p.ink)
            .line_height(Some(44.1)),
    );
    // .lede: 15 px, line-height 1.55, at most 54ch, which Chrome resolves to 405 px for IBM Plex Sans.
    widgets::paragraph(ui, lede, FontId::proportional(15.0), p.muted, 1.55, 405.0);
    // margin-bottom: 6px on top of the card's 14 px gap.
    ui.add_space(6.0);
}

fn actions(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.add_space(10.0);
    let size = Vec2::new(ui.available_width(), 38.0);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            add(ui);
        },
    );
}

fn step_welcome(ui: &mut Ui, p: &Palette, d: &Detection, step: &mut usize) {
    heading(
        ui,
        p,
        "Welcome to Lemmata",
        "Write LaTeX on the left, read the PDF on the right, all on your own computer. Your projects stay ordinary folders.",
    );
    let distribution = match &d.distribution {
        Distribution::TexLive { year } => format!(
            "TeX Live {}",
            year.map(|y| y.to_string()).unwrap_or_default()
        ),
        Distribution::Miktex { version } => {
            format!("MiKTeX {}", version.clone().unwrap_or_default())
        }
        Distribution::Unknown => "TeX".into(),
    };
    egui::Frame::new()
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::R_LG)
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                icons::show(ui, icons::CIRCLE_CHECK, 20.0, p.ok);
                let mut text = Rich::new()
                    .add(
                        distribution.trim(),
                        FontId::new(13.0, theme::font::sans_600()),
                        p.ink,
                    )
                    .add(" is ready", FontId::proportional(13.0), p.ink);
                if let Some(bin) = &d.bin_dir {
                    text = text.add(
                        &format!(" in {}", bin.display()),
                        FontId::monospace(12.0),
                        p.muted,
                    );
                }
                text.show(ui);
            });
            // Only listed for MiKTeX on Windows, which doesn't ship the Perl latexmk runs on.
            if d.tools.iter().any(|t| t.name == "perl") && !found(d, "perl") {
                egui::Frame::new()
                    .fill(Palette::mix(p.warn, 14.0))
                    .corner_radius(theme::R_SM)
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        Rich::new()
                            .add(
                                "Perl is missing.",
                                FontId::new(13.0, theme::font::sans_600()),
                                p.ink,
                            )
                            .add(
                                " latexmk needs it and MiKTeX doesn't include it. Run ",
                                FontId::proportional(13.0),
                                p.ink,
                            )
                            .add(
                                "winget install StrawberryPerl.StrawberryPerl",
                                FontId::monospace(13.0),
                                p.ink,
                            )
                            .add(
                                " in a terminal, then restart Lemmata.",
                                FontId::proportional(13.0),
                                p.ink,
                            )
                            .line_height(13.0 * 1.45)
                            .wrap(ui.available_width())
                            .show(ui);
                    });
            }
            // Two columns of optional tools, indented under the check.
            ui.horizontal(|ui| {
                ui.add_space(30.0);
                let col = (ui.available_width() - 16.0) / 2.0;
                egui::Grid::new("extras")
                    .spacing([16.0, 6.0])
                    .min_col_width(col)
                    .max_col_width(col)
                    .show(ui, |ui| {
                        for (i, (tool, what)) in EXTRAS.iter().enumerate() {
                            let ok = found(d, tool);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let icon = if ok {
                                    icons::CIRCLE_CHECK
                                } else {
                                    icons::CIRCLE
                                };
                                icons::show(ui, icon, 15.0, if ok { p.ok } else { p.faint });
                                ui.label(egui::RichText::new(*what).size(12.5).color(p.ink_2));
                                if !ok {
                                    ui.label(
                                        egui::RichText::new("not installed, optional")
                                            .size(11.5)
                                            .color(p.faint),
                                    );
                                }
                            });
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    actions(ui, |ui| {
        if Button::new("Continue")
            .trailing_icon(icons::ARROW_RIGHT)
            .kind(Kind::Primary)
            .size(Size::Lg)
            .show(ui, p)
            .clicked()
        {
            *step = 1;
        }
    });
}

fn setting_row(ui: &mut Ui, p: &Palette, first: bool, label: &str, add: impl FnOnce(&mut Ui)) {
    if !first {
        let y = ui.cursor().top();
        ui.painter()
            .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(140.0, 30.0), Sense::hover());
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        widgets::label(&mut child, p, label);
        add(ui);
    });
    ui.add_space(10.0);
}

fn step_settings(
    ui: &mut Ui,
    p: &Palette,
    step: &mut usize,
    s: &mut Settings,
    action: &mut Option<Action>,
) {
    heading(
        ui,
        p,
        "Make it yours",
        "All optional, and you can change them later in Preferences.",
    );
    ui.spacing_mut().item_spacing.y = 4.0;
    let mut changed = false;
    setting_row(ui, p, true, "Appearance", |ui| {
        let options = [
            (Some(icons::MONITOR), "System"),
            (Some(icons::SUN), "Light"),
            (Some(icons::MOON), "Dark"),
        ];
        let current = ["system", "light", "dark"]
            .iter()
            .position(|t| *t == s.theme)
            .unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, p, &options, current, false, false) {
            s.theme = ["system", "light", "dark"][i].into();
            changed = true;
        }
    });
    setting_row(ui, p, false, "Save", |ui| {
        let current = if s.autosave_ms > 0.0 { 0 } else { 1 };
        if let Some(i) = widgets::segmented(
            ui,
            p,
            &[(None, "As I type"), (None, "With Ctrl+S")],
            current,
            false,
            false,
        ) {
            s.autosave_ms = if i == 0 {
                Settings::default().autosave_ms
            } else {
                0.0
            };
            changed = true;
        }
    });
    setting_row(ui, p, false, "Spell check", |ui| {
        let mut options: Vec<(&str, &str)> = vec![("auto", "Match the document's language")];
        options.extend(LANGUAGES.iter().copied());
        options.push(("off", "Off"));
        let r = ui.allocate_ui(Vec2::new(300.0, 32.0), |ui| {
            widgets::select(ui, p, "Spell check", &mut s.spell_default, &options)
        });
        changed |= r.inner;
    });
    if changed {
        *action = Some(Action::Settings);
    }
    ui.spacing_mut().item_spacing.y = 14.0;
    actions(ui, |ui| {
        if Button::new("Continue")
            .trailing_icon(icons::ARROW_RIGHT)
            .kind(Kind::Primary)
            .size(Size::Lg)
            .show(ui, p)
            .clicked()
        {
            *step = 2;
        }
        if Button::new("Back").kind(Kind::Ghost).show(ui, p).clicked() {
            *step = 0;
        }
    });
}

fn step_start(ui: &mut Ui, p: &Palette, step: &mut usize, action: &mut Option<Action>) {
    heading(
        ui,
        p,
        "Start writing",
        "Pick up an existing project, or begin a new one.",
    );
    let tiles = [
        (
            icons::PLUS,
            "New project",
            "From a template: article, thesis, slides, CV, letter, homework, or your own.",
            Action::NewProject,
        ),
        (
            icons::FOLDER_OPEN,
            "Open folder",
            "Any folder with a .tex file.",
            Action::OpenFolder,
        ),
        (
            icons::FILE_ARCHIVE,
            "Import from Overleaf",
            "One project, or several at once.",
            Action::Import,
        ),
    ];
    let w = (ui.available_width() - 20.0) / 3.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for (icon, title, body, act) in tiles {
            if start_tile(ui, p, w, icon, title, body).clicked() {
                *action = Some(act);
            }
        }
    });
    // .example: a quiet link-like button.
    let r = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            icons::show(ui, icons::SPARKLES, 15.0, p.accent);
            Rich::new()
                .add(
                    "New to LaTeX or Lemmata? ",
                    FontId::proportional(13.0),
                    p.muted,
                )
                .add(
                    "Open an example document",
                    FontId::proportional(13.0),
                    p.accent,
                )
                .underline()
                .show(ui)
        })
        .response
        .interact(Sense::click());
    if r.clicked() {
        *action = Some(Action::Example);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
        .widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Open an example document")
        });
    actions(ui, |ui| {
        if Button::new("Go to my projects").show(ui, p).clicked() {
            *action = Some(Action::Done);
        }
        if Button::new("Back").kind(Kind::Ghost).show(ui, p).clicked() {
            *step = 1;
        }
    });
}

/// `.start`: an icon in an accent tile, a bold title and a muted line; lifts on hover.
fn start_tile(
    ui: &mut Ui,
    p: &Palette,
    width: f32,
    icon: icons::Icon,
    title: &str,
    body: &str,
) -> egui::Response {
    let id = ui.next_auto_id();
    let hovered = ui.ctx().animate_bool_with_time(
        id,
        ui.ctx().read_response(id).is_some_and(|r| r.hovered()),
        theme::EASE_SECS,
    );
    let body_galley = ui.painter().layout(
        body.into(),
        FontId::proportional(12.5),
        p.muted,
        width - 32.0,
    );
    let height = 16.0 + 36.0 + 6.0 + 4.0 + 18.0 + 4.0 + body_galley.size().y + 16.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let rect = rect.translate(Vec2::new(0.0, -hovered));
    let painter = ui.painter();
    if hovered > 0.0 {
        painter.add(p.shadow.as_shape(rect, CornerRadius::same(theme::R_LG)));
    }
    painter.rect(
        rect,
        CornerRadius::same(theme::R_LG),
        p.surface,
        Stroke::new(1.0, p.border.lerp_to_gamma(p.accent, hovered)),
        StrokeKind::Inside,
    );
    let ico = egui::Rect::from_min_size(rect.min + Vec2::splat(16.0), Vec2::splat(36.0));
    painter.rect_filled(ico, CornerRadius::same(theme::R), p.accent_soft);
    icons::paint(painter, ico.shrink(8.0), icon, p.accent);
    let mut y = ico.bottom() + 6.0 + 4.0;
    painter.text(
        pos2(rect.left() + 16.0, y),
        egui::Align2::LEFT_TOP,
        title,
        FontId::new(13.0, theme::font::sans_600()),
        p.ink,
    );
    y += 18.0 + 4.0;
    painter.galley(
        pos2(rect.left() + 16.0, y),
        body_galley,
        Color32::PLACEHOLDER,
    );
    widgets::focus_ring(ui, &response, rect, p, theme::R_LG);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    fn render(p: Palette, step: usize, name: &str) -> egui_kittest::SnapshotResult {
        let d = testing::detection();
        let mut settings = Settings::default();
        let mut step = step;
        let mut h = testing::harness(p, move |ui| {
            show(ui, &p, &d, &mut step, &mut settings);
        });
        h.run();
        h.try_snapshot(name)
    }

    #[test]
    fn welcome_steps_light_and_dark() {
        let mut results = egui_kittest::SnapshotResults::new();
        for (p, scheme) in [(theme::LIGHT, "light"), (theme::DARK, "dark")] {
            for step in 0..3 {
                results.add(render(p, step, &format!("welcome-{}-{scheme}", step + 1)));
            }
        }
    }
}
