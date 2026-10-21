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
            tectonic_supported: true,
            tectonic_mb: 30,
        }
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
            ui.label(
                egui::RichText::new("Lemmata")
                    .font(FontId::new(22.0, theme::font::display()))
                    .color(p.ink),
            );
            ui.add_space(0.05 * full.height());
            widgets::card(
                ui,
                p,
                14,
                Margin {
                    left: 36,
                    right: 36,
                    top: 32,
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
        Rich::new()
            .add(
                "Installer and instructions: ",
                FontId::proportional(13.0),
                p.muted,
            )
            .add(link, FontId::monospace(13.0), p.muted)
            .wrap(ui.available_width())
            .show(ui);
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

/// Each tool as a small tile: found, or required (red), or optional.
fn checks(ui: &mut Ui, p: &Palette, status: &TexStatus) {
    let tools = &status.detection.tools;
    let w = ui.available_width();
    // grid-template-columns: repeat(auto-fill, minmax(100px, 1fr)); gap: 8px
    let cols = (((w + 8.0) / 108.0).floor() as usize).max(1);
    let cell = (w - 8.0 * (cols as f32 - 1.0)) / cols as f32;
    let rows = tools.len().div_ceil(cols);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(
            w,
            rows as f32 * 53.0 + (rows.saturating_sub(1)) as f32 * 8.0,
        ),
        Sense::hover(),
    );
    let painter = ui.painter();
    for (i, t) in tools.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let tile = egui::Rect::from_min_size(
            rect.min + Vec2::new(c as f32 * (cell + 8.0), r as f32 * 61.0),
            Vec2::new(cell, 53.0),
        );
        let missing_required = t.path.is_none() && t.required;
        let (fill, border) = if missing_required {
            (p.error_soft, p.error.lerp_to_gamma(p.border, 0.6))
        } else {
            (p.surface_2, p.border)
        };
        painter.rect(
            tile,
            CornerRadius::same(theme::R),
            fill,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        let (icon, icon_color) = match (&t.path, t.required) {
            (Some(_), _) => (icons::CIRCLE_CHECK, p.ok),
            (None, true) => (icons::CIRCLE_X, p.error),
            (None, false) => (icons::CIRCLE_X, p.faint),
        };
        let top = tile.min + Vec2::new(11.0, 9.0);
        icons::paint(
            painter,
            egui::Rect::from_min_size(top + Vec2::new(0.0, 2.0), Vec2::splat(14.0)),
            icon,
            icon_color,
        );
        painter.text(
            top + Vec2::new(20.0, 0.0),
            egui::Align2::LEFT_TOP,
            &t.name,
            FontId::new(12.5, theme::font::mono_500()),
            p.ink,
        );
        let state = if t.path.is_some() {
            "found"
        } else if t.required {
            "required"
        } else {
            "optional"
        };
        painter.text(
            top + Vec2::new(0.0, 20.0),
            egui::Align2::LEFT_TOP,
            state,
            FontId::proportional(11.5),
            p.muted,
        );
    }
}

/// One tab per platform, this computer's marked.
fn tabs(ui: &mut Ui, p: &Palette, status: &TexStatus, st: &mut State) {
    let here = status.os.family.key();
    let start = ui.cursor().min;
    // The rule under the tabs goes behind them, so the active tab's accent line covers it.
    let rule = ui.painter().add(egui::Shape::Noop);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for key in ORDER
            .iter()
            .filter(|k| status.instructions.contains_key(**k))
        {
            let label = &status.instructions[*key].label;
            let on = st.platform == *key;
            let mut job = Rich::new().add(
                label,
                FontId::new(13.0, theme::font::sans_500()),
                if on { p.ink } else { p.muted },
            );
            if *key == here {
                job = job.add(
                    " · this computer",
                    FontId::new(13.0, theme::font::sans_500()),
                    p.accent,
                );
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job.0));
            let (rect, r) =
                ui.allocate_exact_size(Vec2::new(galley.size().x + 24.0, 36.0), Sense::click());
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, label));
            ui.painter().galley(
                pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
                galley,
                p.ink,
            );
            if on {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), rect.max),
                    0,
                    p.accent,
                );
            }
            if r.clicked() {
                st.platform = (*key).into();
            }
        }
    });
    let y = start.y + 36.0 - 0.5;
    let line = egui::Rect::from_min_max(
        pos2(ui.max_rect().left(), y),
        pos2(ui.max_rect().right(), y + 1.0),
    );
    ui.painter()
        .set(rule, egui::Shape::rect_filled(line, 0, p.border));
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
            let (rect, r) = ui.allocate_exact_size(Vec2::new(w, 64.0), Sense::click());
            let on = st.full == is_full;
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &title));
            let painter = ui.painter();
            if on {
                painter.rect_stroke(
                    rect.expand(1.5),
                    CornerRadius::same(theme::R_LG + 2),
                    Stroke::new(3.0, Palette::mix(p.accent, 18.0)),
                    StrokeKind::Outside,
                );
            }
            painter.rect(
                rect,
                CornerRadius::same(theme::R_LG),
                p.surface,
                Stroke::new(1.0, if on { p.accent } else { p.border }),
                StrokeKind::Inside,
            );
            painter.text(
                rect.min + Vec2::new(14.0, 13.0),
                egui::Align2::LEFT_TOP,
                &title,
                FontId::new(13.0, theme::font::sans_600()),
                p.ink,
            );
            painter.text(
                rect.min + Vec2::new(14.0, 35.0),
                egui::Align2::LEFT_TOP,
                body,
                FontId::proportional(12.5),
                p.muted,
            );
            widgets::focus_ring(ui, &r, rect, p, theme::R_LG);
            if r.clicked() {
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
    let frame = egui::Frame::new()
        .fill(p.surface_2)
        .corner_radius(theme::R_LG)
        .inner_margin(Margin::symmetric(14, 12));
    let r = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            let label = if st.installing { "Downloading…" } else { "Use Tectonic (no install)" };
            let btn_w = ui.fonts_mut(|f| f.layout_no_wrap(label.into(), FontId::new(13.0, theme::font::sans_500()), p.ink).size().x) + 24.0;
            let text_w = (ui.available_width() - btn_w - 16.0).min(62.0 * 6.25);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - btn_w - 16.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(egui::RichText::new("Or use Tectonic: nothing to install").font(FontId::new(13.0, theme::font::sans_600())).color(p.ink));
                let body = format!(
                    "A small LaTeX engine. Lemmata downloads it (~{} MB) and it fetches each package the first time a document needs it, so the first build needs an internet connection. biblatex with biber isn't supported; use BibTeX.",
                    st.tectonic_mb
                );
                widgets::paragraph(ui, &body, FontId::proportional(12.5), p.muted, 1.45, text_w);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Button::new(label).enabled(!st.installing && !checking).show(ui, p).clicked() {
                    *action = Some(Action::UseTectonic);
                }
            });
        });
    });
    dashed_rect(ui.painter(), r.response.rect, p.border);
}

/// `border: 1px dashed`, with rounded corners left out as CSS draws them.
fn dashed_rect(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let r = rect.shrink(0.5);
    let k = theme::R_LG as f32;
    let stroke = Stroke::new(1.0, color);
    let sides = [
        [pos2(r.left() + k, r.top()), pos2(r.right() - k, r.top())],
        [
            pos2(r.left() + k, r.bottom()),
            pos2(r.right() - k, r.bottom()),
        ],
        [pos2(r.left(), r.top() + k), pos2(r.left(), r.bottom() - k)],
        [
            pos2(r.right(), r.top() + k),
            pos2(r.right(), r.bottom() - k),
        ],
    ];
    for side in sides {
        painter.extend(egui::Shape::dashed_line(&side, stroke, 3.0, 3.0));
    }
    for (c, a0) in [
        (r.left_top() + Vec2::splat(k), 180.0f32),
        (r.right_top() + Vec2::new(-k, k), 270.0),
        (r.right_bottom() - Vec2::splat(k), 0.0),
        (r.left_bottom() + Vec2::new(k, -k), 90.0),
    ] {
        let pts: Vec<_> = (0..=6)
            .map(|i| c + Vec2::angled((a0 + 15.0 * i as f32).to_radians()) * k)
            .collect();
        painter.add(egui::Shape::line(pts, stroke));
    }
}

fn footer(ui: &mut Ui, p: &Palette, checking: bool, can_skip: bool, action: &mut Option<Action>) {
    ui.add_space(6.0);
    let y = ui.cursor().top();
    ui.painter()
        .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    ui.add_space(16.0);
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 38.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            });
        },
    );
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
