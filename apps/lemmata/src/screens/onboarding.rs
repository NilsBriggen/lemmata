//! Shown when no usable TeX installation was found (Onboarding.svelte). Lemmata never runs the
//! install itself: it shows the command for this system and checks again.

use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Rich, Size};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
};
use lemmata_shell::TexStatus;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub enum Action {
    /// Check again, with a folder the user chose (already saved as `texDir`) or without.
    Recheck(Option<String>),
    ChooseFolder,
    /// Install the platform's recommended or full TeX in a terminal.
    OpenTerminal {
        platform: String,
        full: bool,
    },
    UseTectonic,
    /// Open an external link (installer / instructions URL).
    OpenUrl(String),
    /// Open the app anyway: projects can be edited, nothing compiles until TeX is found.
    Skip,
}

const ORDER: [&str; 7] = [
    "arch", "debian", "fedora", "suse", "macos", "windows", "other",
];

pub struct State {
    /// Starts on this computer's platform; the user can switch tabs.
    pub platform: String,
    pub full: bool,
    copied: Option<Instant>,
    pub installing: bool,
    pub tectonic_error: Option<String>,
    pub terminal_error: Option<String>,
    pub tectonic_supported: bool,
    pub tectonic_mb: u64,
}

impl State {
    pub fn new(status: &TexStatus) -> Self {
        Self {
            platform: status.os.family.key().into(),
            full: false,
            copied: None,
            installing: false,
            tectonic_error: None,
            terminal_error: None,
            tectonic_supported: lemmata_tex::tectonic::release().is_some(),
            // This computer's download, rounded: about 10 MB on Linux, 22 MB elsewhere.
            tectonic_mb: lemmata_tex::tectonic::release()
                .map_or(20, |r| r.size.div_ceil(1_000_000)),
        }
    }

    pub fn refresh(&mut self, _status: &TexStatus) {
        self.tectonic_supported = lemmata_tex::tectonic::release().is_some();
        self.tectonic_mb =
            lemmata_tex::tectonic::release().map_or(20, |r| r.size.div_ceil(1_000_000));
    }
}

pub fn show(
    ui: &mut Ui,
    p: &Palette,
    status: &TexStatus,
    checking: bool,
    can_skip: bool,
    st: &mut State,
) -> Option<Action> {
    let mut action = None;
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
        ui.add_space(24.0);
        widgets::column(ui, 860.0, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let _ = widgets::logo_mark(ui, 28.0);
                ui.label(
                    egui::RichText::new("Lemmata")
                        .font(FontId::new(22.0, theme::font::display()))
                        .color(p.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    for (idx, lbl) in [(2, "Start"), (1, "Preferences"), (0, "TeX engine")] {
                        let on = idx == 0;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                            ui.painter().circle_filled(
                                dot.center(),
                                3.5,
                                if on { p.accent } else { p.border_strong },
                            );
                            ui.label(
                                egui::RichText::new(lbl)
                                    .font(FontId::new(12.0, theme::font::sans_500()))
                                    .color(if on { p.ink } else { p.faint }),
                            );
                        });
                    }
                });
            });
            ui.add_space(0.04 * full.height());
            widgets::card(
                ui,
                p,
                theme::R_XL,
                Margin {
                    left: 36,
                    right: 36,
                    top: 30,
                    bottom: 26,
                },
                |ui| {
                    ui.spacing_mut().item_spacing.y = 16.0;
                    body(ui, p, status, checking, can_skip, st, &mut action);
                },
            );
            ui.add_space(40.0);
        });
    });
    // Keeps "Copied" and the spinner moving without input.
    if checking
        || st
            .copied
            .is_some_and(|t| t.elapsed() < Duration::from_millis(1500))
    {
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }
    action
}

fn body(
    ui: &mut Ui,
    p: &Palette,
    status: &TexStatus,
    checking: bool,
    can_skip: bool,
    st: &mut State,
    action: &mut Option<Action>,
) {
    let d = &status.detection;
    let found = d.tools.iter().filter(|t| t.path.is_some()).count();
    let mut eyebrow = egui::text::LayoutJob::default();
    eyebrow.append(
        "ONE STEP BEFORE YOU START",
        0.0,
        egui::TextFormat {
            font_id: FontId::new(11.0, theme::font::sans_600()),
            color: p.accent,
            extra_letter_spacing: 0.08 * 11.0,
            ..Default::default()
        },
    );
    ui.label(eyebrow);
    ui.add_space(-6.0);
    let title = if found == 0 {
        "TeX Live isn't installed yet"
    } else {
        "TeX Live is incomplete"
    };
    ui.label(
        egui::RichText::new(title)
            .font(FontId::new(30.0, theme::font::display()))
            .color(p.ink)
            .line_height(Some(41.3)),
    );
    let lede = format!(
        "Lemmata uses the TeX installation on your computer to build PDFs. We looked in your PATH and the usual install folders{}. Install it with the command below, then click Check again.",
        if found == 0 {
            " and found nothing"
        } else {
            ", but some required tools are missing"
        }
    );
    // 64ch at 14.5 px, as Chrome resolves ch for IBM Plex Sans (0.5 em).
    widgets::paragraph(
        ui,
        &lede,
        FontId::proportional(14.5),
        p.muted,
        1.55,
        64.0 * 7.25,
    );

    checks(ui, p, status);
    tabs(ui, p, status, st);

    let inst = status
        .instructions
        .get(&st.platform)
        .or_else(|| status.instructions.get("other"));
    let Some(inst) = inst else { return };
    if inst.recommended != inst.full {
        sizes(ui, p, st, &inst.recommended_size, &inst.full_size);
    }
    let command = if st.full {
        &inst.full
    } else {
        &inst.recommended
    };
    if !command.starts_with("Install ") {
        if command_box(ui, p, command, &mut st.copied) {
            *action = Some(Action::OpenTerminal {
                platform: st.platform.clone(),
                full: st.full,
            });
        }
        if let Some(e) = &st.terminal_error {
            ui.add_space(-12.0);
            ui.label(egui::RichText::new(e).size(12.0).color(p.error));
        }
    } else {
        note(ui, p, command);
    }
    if let Some(n) = &inst.note {
        note(ui, p, n);
    }
    if let Some(link) = &inst.link {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(
                egui::RichText::new("Installer and instructions: ")
                    .font(FontId::proportional(13.0))
                    .color(p.muted),
            );
            let r = ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(link)
                            .font(FontId::monospace(13.0))
                            .color(p.accent)
                            .underline(),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Open in browser");
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, link));
            if r.clicked() {
                *action = Some(Action::OpenUrl(link.clone()));
            }
        });
    }
    note(
        ui,
        p,
        "Open terminal shows the command and waits for you to press Enter. Lemmata never installs TeX Live by itself.",
    );

    if st.tectonic_supported {
        tectonic(ui, p, st, checking, action);
        if let Some(e) = &st.tectonic_error {
            ui.label(egui::RichText::new(e).color(p.error));
        }
    }
    footer(ui, p, checking, can_skip, action);
}

fn note(ui: &mut Ui, p: &Palette, text: &str) {
    widgets::paragraph(
        ui,
        text,
        FontId::proportional(13.0),
        p.muted,
        1.45,
        ui.available_width(),
    );
}

/// Each tool as a compact status chip: found (green check), missing required (red), or optional.
fn checks(ui: &mut Ui, p: &Palette, status: &TexStatus) {
    let tools = &status.detection.tools;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
        for t in tools {
            let (icon, bg, border, fg, state) = match (&t.path, t.required) {
                (Some(_), _) => (
                    icons::CHECK,
                    p.ok_soft,
                    p.ok.gamma_multiply(0.35),
                    p.ok,
                    "found",
                ),
                (None, true) => (
                    icons::CIRCLE_X,
                    p.error_soft,
                    p.error.gamma_multiply(0.35),
                    p.error,
                    "required",
                ),
                (None, false) => (icons::CIRCLE, p.surface_2, p.border, p.muted, "optional"),
            };
            let ng = ui.painter().layout_no_wrap(
                t.name.clone(),
                FontId::new(12.0, theme::font::mono_500()),
                fg,
            );
            let sg =
                ui.painter()
                    .layout_no_wrap(format!("· {state}"), FontId::proportional(11.5), fg);
            let w = 10.0 + 13.0 + 6.0 + ng.size().x + 5.0 + sg.size().x + 10.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
            let painter = ui.painter();
            painter.rect(
                rect,
                CornerRadius::same(theme::R_MD),
                bg,
                Stroke::new(1.0, border),
                StrokeKind::Inside,
            );
            let mut x = rect.left() + 10.0;
            icons::paint(
                painter,
                egui::Rect::from_min_size(pos2(x, rect.center().y - 6.5), Vec2::splat(13.0)),
                icon,
                fg,
            );
            x += 19.0;
            painter.galley(pos2(x, rect.center().y - ng.size().y / 2.0), ng, fg);
            let x2 = rect.right() - 10.0 - sg.size().x;
            painter.galley(
                pos2(x2, rect.center().y - sg.size().y / 2.0),
                sg,
                fg.gamma_multiply(0.85),
            );
        }
    });
}

/// OS picker using the modern segmented track.
fn tabs(ui: &mut Ui, p: &Palette, status: &TexStatus, st: &mut State) {
    let here = status.os.family.key();
    let keys: Vec<&str> = ORDER
        .iter()
        .copied()
        .filter(|k| status.instructions.contains_key(*k))
        .collect();
    let labels: Vec<String> = keys
        .iter()
        .map(|k| {
            let lbl = &status.instructions[*k].label;
            if *k == here {
                format!("{lbl} · this computer")
            } else {
                lbl.clone()
            }
        })
        .collect();
    let opts: Vec<(Option<icons::Icon>, &str)> =
        labels.iter().map(|l| (None, l.as_str())).collect();
    let cur = keys.iter().position(|k| *k == st.platform).unwrap_or(0);
    if let Some(i) = widgets::segmented(ui, p, &opts, cur, false) {
        st.platform = keys[i].into();
    }
}

fn sizes(ui: &mut Ui, p: &Palette, st: &mut State, recommended: &str, full: &str) {
    let w = (ui.available_width() - 10.0) / 2.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for (is_full, title, body) in [
            (
                false,
                format!("Recommended {recommended}"),
                "Covers almost every paper, thesis and Beamer deck",
            ),
            (
                true,
                format!("Full {full}"),
                "Every package, so you're never asked to install more",
            ),
        ] {
            let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 66.0), Sense::click());
            let on = st.full == is_full;
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &title));
            let painter = ui.painter();
            let rad = CornerRadius::same(theme::R_MD);
            if on {
                painter.rect_stroke(
                    rect.expand(1.5),
                    CornerRadius::same(theme::R_MD + 2),
                    Stroke::new(3.0, Palette::mix(p.accent, 18.0)),
                    StrokeKind::Outside,
                );
            }
            painter.rect(
                rect,
                rad,
                if on { p.accent_soft } else { p.surface },
                Stroke::new(
                    if on { 1.5 } else { 1.0 },
                    if on { p.accent } else { p.border },
                ),
                StrokeKind::Inside,
            );
            // Radio dot on left.
            let radio = pos2(rect.left() + 18.0, rect.top() + 21.0);
            painter.circle_stroke(
                radio,
                7.0,
                Stroke::new(1.5, if on { p.accent } else { p.border_strong }),
            );
            if on {
                painter.circle_filled(radio, 3.8, p.accent);
            }
            painter.text(
                rect.min + Vec2::new(34.0, 13.0),
                egui::Align2::LEFT_TOP,
                &title,
                FontId::new(13.0, theme::font::sans_600()),
                p.ink,
            );
            painter.text(
                rect.min + Vec2::new(34.0, 35.0),
                egui::Align2::LEFT_TOP,
                body,
                FontId::proportional(12.0),
                p.muted,
            );
            widgets::focus_ring(ui, &r, rect, p, theme::R_MD);
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                st.full = is_full;
            }
        }
    });
}

/// CommandBox.svelte: the command on a dark box with Copy and Open terminal. True when Open terminal was clicked.
pub fn command_box(ui: &mut Ui, p: &Palette, command: &str, copied: &mut Option<Instant>) -> bool {
    const BOX: Color32 = Color32::from_rgb(0x0f, 0x14, 0x18);
    const INK: Color32 = Color32::from_rgb(0xd5, 0xdd, 0xe3);
    let mut open = false;
    egui::Frame::new()
        .fill(BOX)
        .corner_radius(theme::R)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let showing_copied =
                    copied.is_some_and(|t| t.elapsed() < Duration::from_millis(1500));
                let labels = [
                    if showing_copied { "Copied" } else { "Copy" },
                    "Open terminal",
                ];
                let font = FontId::proportional(13.0);
                let widths: Vec<f32> = labels
                    .iter()
                    .map(|l| {
                        ui.fonts_mut(|f| f.layout_no_wrap((*l).into(), font.clone(), INK).size().x)
                            + 14.0
                    })
                    .collect();
                let text_w = ui.available_width() - widths.iter().sum::<f32>() - 6.0 - 12.0;
                Rich::new()
                    .add(command, FontId::monospace(12.5), INK)
                    .line_height(12.5 * 1.6)
                    .wrap(text_w)
                    .show(ui);
                ui.spacing_mut().item_spacing.x = 6.0;
                for (i, l) in labels.iter().enumerate() {
                    let (rect, r) =
                        ui.allocate_exact_size(Vec2::new(widths[i], 24.0), Sense::click());
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, *l));
                    let hover = r.hovered();
                    ui.painter().rect(
                        rect,
                        CornerRadius::same(2),
                        if hover {
                            Color32::from_rgb(0x26, 0x30, 0x39)
                        } else {
                            Color32::from_rgb(0x1d, 0x25, 0x2c)
                        },
                        Stroke::new(1.0, Color32::from_rgb(0x2c, 0x34, 0x3d)),
                        StrokeKind::Inside,
                    );
                    widgets::centered_text(ui, rect, l, font.clone(), INK);
                    widgets::focus_ring(ui, &r, rect, p, 2);
                    if r.clicked() {
                        if i == 0 {
                            ui.ctx().copy_text(command.into());
                            *copied = Some(Instant::now());
                        } else {
                            open = true;
                        }
                    }
                }
            });
        });
    open
}

fn tectonic(ui: &mut Ui, p: &Palette, st: &mut State, checking: bool, action: &mut Option<Action>) {
    egui::Frame::new()
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::R_LG)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let label = if st.installing {
                    "Downloading…"
                } else {
                    "Use Tectonic (no install)"
                };
                let btn_w = ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        label.into(),
                        FontId::new(13.0, theme::font::sans_500()),
                        p.ink,
                    )
                    .size()
                    .x
                }) + 24.0;
                let text_w = (ui.available_width() - btn_w - 16.0).min(62.0 * 6.25);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - btn_w - 16.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        egui::RichText::new("Or use Tectonic: nothing to install")
                            .font(FontId::new(13.0, theme::font::sans_600()))
                            .color(p.ink),
                    );
                    let body = format!(
                        "A small LaTeX engine. Lemmata downloads it (~{} MB) and it fetches each package the first time a document needs it, so the first build needs an internet connection. biblatex with biber isn't supported; use BibTeX.",
                        st.tectonic_mb
                    );
                    widgets::paragraph(ui, &body, FontId::proportional(12.5), p.muted, 1.45, text_w);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if Button::new(label)
                        .enabled(!st.installing && !checking)
                        .show(ui, p)
                        .clicked()
                    {
                        *action = Some(Action::UseTectonic);
                    }
                });
            });
        });
}

fn footer(ui: &mut Ui, p: &Palette, checking: bool, can_skip: bool, action: &mut Option<Action>) {
    ui.add_space(6.0);
    let y = ui.cursor().top();
    ui.painter()
        .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    ui.add_space(16.0);
    let draw_choose = |ui: &mut Ui, action: &mut Option<Action>| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(egui::RichText::new("Installed somewhere else? ").color(p.muted));
        let link = ui.add(
            egui::Label::new(
                egui::RichText::new("Choose the TeX bin folder")
                    .color(p.accent)
                    .underline(),
            )
            .sense(Sense::click()),
        );
        link.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Choose the TeX bin folder")
        });
        if link
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            *action = Some(Action::ChooseFolder);
        }
    };
    let draw_buttons = |ui: &mut Ui, action: &mut Option<Action>| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let label = if checking {
            "Checking…"
        } else {
            "Check again"
        };
        let angle = if checking {
            ui.input(|i| i.time) as f32 * std::f32::consts::TAU / 0.9
        } else {
            0.0
        };
        if Button::new(label)
            .with_icon(icons::REFRESH_CW)
            .icon_angle(angle)
            .kind(Kind::Primary)
            .size(Size::Lg)
            .enabled(!checking)
            .show(ui, p)
            .clicked()
        {
            *action = Some(Action::Recheck(None));
        }
        if can_skip
            && Button::new("Continue without compiling")
                .size(Size::Lg)
                .show(ui, p)
                .clicked()
        {
            *action = Some(Action::Skip);
        }
    };
    if can_skip && ui.available_width() < 700.0 {
        ui.horizontal_wrapped(|ui| draw_choose(ui, action));
        ui.add_space(10.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 38.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| draw_buttons(ui, action),
        );
    } else {
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 38.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                draw_choose(ui, action);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_buttons(ui, action);
                });
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use lemmata_tex::os::{Family, OsInfo};

    /// The mock's no-TeX scenario: nothing found, on Arch.
    pub fn no_tex() -> TexStatus {
        let mut detection = testing::detection();
        for t in &mut detection.tools {
            t.path = None;
        }
        detection.bin_dir = None;
        detection.ready = false;
        detection.tex_ready = false;
        let mut instructions = lemmata_tex::os::all_instructions();
        // The mock lists these five platforms.
        instructions
            .retain(|k, _| ["arch", "debian", "macos", "windows", "other"].contains(&k.as_str()));
        // The mock's Arch sizes, so the snapshot matches the reference.
        if let Some(a) = instructions.get_mut("arch") {
            a.recommended_size = "≈1.2 GB".into();
            a.full_size = "≈5.6 GB".into();
        }
        TexStatus {
            detection,
            os: OsInfo {
                family: Family::Arch,
                name: "CachyOS".into(),
            },
            instructions,
        }
    }

    #[test]
    fn onboarding_light_and_dark() {
        let mut results = egui_kittest::SnapshotResults::new();
        for (p, scheme) in [(theme::LIGHT, "light"), (theme::DARK, "dark")] {
            let status = no_tex();
            let mut st = State::new(&status);
            st.tectonic_mb = 11;
            let mut h = testing::harness(p, move |ui| {
                show(ui, &p, &status, false, false, &mut st);
            });
            h.run();
            results.add(h.try_snapshot(format!("onboarding-{scheme}")));
        }
    }
}
