//! Shown when no usable TeX installation was found (`Onboarding`). Shares the two-column guided
//! setup shell with `welcome.rs` so TeX setup flows seamlessly into first-run preferences.
//! Lemmata never runs the install itself: it shows the command for this system and checks again.

use super::welcome::{self, StepItem};
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

const ONBOARDING_STEPS: [StepItem; 4] = [
    StepItem {
        title: "TeX engine",
        subtitle: "Install or locate TeX",
    },
    StepItem {
        title: "Appearance",
        subtitle: "Theme & typography",
    },
    StepItem {
        title: "Workflow",
        subtitle: "Save, spell & helpers",
    },
    StepItem {
        title: "Start",
        subtitle: "Your first project",
    },
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
    let mut body_action = None;
    let mut foot_action = None;
    let (_, skipped) = welcome::guide_shell(
        ui,
        p,
        "onboarding",
        "TeX setup",
        &ONBOARDING_STEPS,
        0,
        false,
        can_skip,
        |ui| {
            body(ui, p, status, checking, st, &mut body_action);
        },
        |ui| {
            footer(ui, p, checking, can_skip, &mut foot_action);
        },
    );
    let mut action = body_action.or(foot_action);
    if skipped {
        action = Some(Action::Skip);
    }
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
    st: &mut State,
    action: &mut Option<Action>,
) {
    ui.spacing_mut().item_spacing.y = 11.0;
    let d = &status.detection;
    let found = d.tools.iter().filter(|t| t.path.is_some()).count();
    let title = if found == 0 {
        "TeX Live isn't installed yet"
    } else {
        "TeX Live is incomplete"
    };
    let lede = format!(
        "Lemmata uses the TeX installation on your computer to build PDFs. We looked in your PATH and the usual install folders{}. Install it with the command below, then click Check again.",
        if found == 0 {
            " and found nothing"
        } else {
            ", but some required tools are missing"
        }
    );
    welcome::step_heading(ui, p, "STEP 1 OF 4 · TEX ENGINE", title, &lede);

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
            ui.add_space(-10.0);
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
                    .font(FontId::proportional(12.5))
                    .color(p.muted),
            );
            let r = ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(link)
                            .font(FontId::monospace(12.5))
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
}

fn note(ui: &mut Ui, p: &Palette, text: &str) {
    widgets::paragraph(
        ui,
        text,
        FontId::proportional(12.5),
        p.muted,
        1.45,
        ui.available_width(),
    );
}

/// Each tool as a compact status chip: found (green check), missing required (red), or optional.
fn checks(ui: &mut Ui, p: &Palette, status: &TexStatus) {
    let tools = &status.detection.tools;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(7.0, 7.0);
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
                FontId::new(11.5, theme::font::mono_500()),
                fg,
            );
            let sg =
                ui.painter()
                    .layout_no_wrap(format!("· {state}"), FontId::proportional(11.0), fg);
            let w = 9.0 + 12.0 + 6.0 + ng.size().x + 5.0 + sg.size().x + 9.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::hover());
            let painter = ui.painter();
            painter.rect(
                rect,
                CornerRadius::same(theme::R_MD),
                bg,
                Stroke::new(1.0, border),
                StrokeKind::Inside,
            );
            let mut x = rect.left() + 9.0;
            icons::paint(
                painter,
                egui::Rect::from_min_size(pos2(x, rect.center().y - 6.0), Vec2::splat(12.0)),
                icon,
                fg,
            );
            x += 18.0;
            painter.galley(pos2(x, rect.center().y - ng.size().y / 2.0), ng, fg);
            let x2 = rect.right() - 9.0 - sg.size().x;
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
    egui::ScrollArea::horizontal()
        .id_salt("onboarding-os-tabs")
        .show(ui, |ui| {
            if let Some(i) = widgets::segmented(ui, p, &opts, cur, false) {
                st.platform = keys[i].into();
            }
        });
}

fn sizes(ui: &mut Ui, p: &Palette, st: &mut State, recommended: &str, full: &str) {
    let w = ((ui.available_width() - 10.0) / 2.0).max(160.0);
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
            let on = st.full == is_full;
            let body_g = ui.painter().layout(
                body.into(),
                FontId::proportional(11.5),
                p.muted,
                (w - 46.0).max(80.0),
            );
            let card_h = (36.0 + body_g.size().y + 12.0).max(66.0);
            let (rect, r) = ui.allocate_exact_size(Vec2::new(w, card_h), Sense::click());
            let sel_t = if cfg!(test) || theme::ease_secs() == 0.0 {
                if on { 1.0 } else { 0.0 }
            } else {
                ui.ctx()
                    .animate_bool_with_time(r.id.with("sel"), on, theme::ease_secs())
            };
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, &title));
            let painter = ui.painter();
            let rad = CornerRadius::same(theme::R_MD);
            if sel_t > 0.01 {
                painter.rect_stroke(
                    rect.expand(1.5),
                    CornerRadius::same(theme::R_MD + 2),
                    Stroke::new(3.0, Palette::mix(p.accent, 18.0 * sel_t)),
                    StrokeKind::Outside,
                );
            }
            painter.rect(
                rect,
                rad,
                p.surface.lerp_to_gamma(p.accent_soft, sel_t),
                Stroke::new(1.0 + 0.5 * sel_t, p.border.lerp_to_gamma(p.accent, sel_t)),
                StrokeKind::Inside,
            );
            // Radio dot on left.
            let radio = pos2(rect.left() + 18.0, rect.top() + 20.0);
            painter.circle_stroke(
                radio,
                7.0,
                Stroke::new(1.5, p.border_strong.lerp_to_gamma(p.accent, sel_t)),
            );
            if sel_t > 0.05 {
                painter.circle_filled(radio, 3.8 * sel_t, p.accent);
            }
            painter.text(
                rect.min + Vec2::new(34.0, 12.0),
                egui::Align2::LEFT_TOP,
                &title,
                FontId::new(12.5, theme::font::sans_600()),
                p.ink,
            );
            painter.galley(
                rect.min + Vec2::new(34.0, 32.0),
                body_g,
                Color32::PLACEHOLDER,
            );
            widgets::focus_ring(ui, &r, rect, p, theme::R_MD);
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                st.full = is_full;
            }
        }
    });
}

/// The install command on a dark terminal box with Copy and Open terminal. True when Open terminal was clicked.
pub fn command_box(ui: &mut Ui, p: &Palette, command: &str, copied: &mut Option<Instant>) -> bool {
    const BOX: Color32 = Color32::from_rgb(0x0f, 0x14, 0x18);
    const INK: Color32 = Color32::from_rgb(0xd5, 0xdd, 0xe3);
    const PROMPT: Color32 = Color32::from_rgb(0x6c, 0x99, 0xbb);
    let mut open = false;
    egui::Frame::new()
        .fill(BOX)
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x25, 0x2f, 0x38)))
        .corner_radius(theme::R_MD)
        .inner_margin(Margin::symmetric(14, 11))
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
                let font = FontId::proportional(12.5);
                let widths: Vec<f32> = labels
                    .iter()
                    .map(|l| {
                        ui.fonts_mut(|f| f.layout_no_wrap((*l).into(), font.clone(), INK).size().x)
                            + 16.0
                    })
                    .collect();
                let text_w = (ui.available_width() - widths.iter().sum::<f32>() - 18.0).max(80.0);
                Rich::new()
                    .add("$ ", FontId::monospace(12.0), PROMPT)
                    .add(command, FontId::monospace(12.0), INK)
                    .line_height(12.0 * 1.6)
                    .wrap(text_w)
                    .show(ui);
                ui.spacing_mut().item_spacing.x = 6.0;
                for (i, l) in labels.iter().enumerate() {
                    let (rect, r) =
                        ui.allocate_exact_size(Vec2::new(widths[i], 26.0), Sense::click());
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, *l));
                    let hover = r.hovered();
                    let is_copied = i == 0 && showing_copied;
                    ui.painter().rect(
                        rect,
                        CornerRadius::same(theme::R_SM),
                        if is_copied {
                            Color32::from_rgb(0x1b, 0x3b, 0x2b)
                        } else if hover {
                            Color32::from_rgb(0x26, 0x30, 0x39)
                        } else {
                            Color32::from_rgb(0x1d, 0x25, 0x2c)
                        },
                        Stroke::new(
                            1.0,
                            if is_copied {
                                Color32::from_rgb(0x2f, 0x7a, 0x52)
                            } else {
                                Color32::from_rgb(0x2c, 0x34, 0x3d)
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    widgets::centered_text(ui, rect, l, font.clone(), INK);
                    widgets::focus_ring(ui, &r, rect, p, theme::R_SM);
                    if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
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
                ui.spacing_mut().item_spacing.x = 14.0;
                let label = if st.installing {
                    "Downloading…"
                } else {
                    "Use Tectonic (no install)"
                };
                let btn_w = ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        label.into(),
                        FontId::new(12.5, theme::font::sans_500()),
                        p.ink,
                    )
                    .size()
                    .x
                }) + 24.0;
                let text_w = (ui.available_width() - btn_w - 14.0).max(120.0);
                ui.vertical(|ui| {
                    ui.set_width(text_w);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        egui::RichText::new("Or use Tectonic: nothing to install")
                            .font(FontId::new(13.0, theme::font::sans_600()))
                            .color(p.ink),
                    );
                    let body = format!(
                        "A small LaTeX engine (~{} MB). Fetches packages automatically on first build (needs internet). biblatex with biber isn't supported; use BibTeX.",
                        st.tectonic_mb
                    );
                    widgets::paragraph(ui, &body, FontId::proportional(12.0), p.muted, 1.45, text_w);
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
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.label(
        egui::RichText::new("Installed elsewhere? ")
            .size(12.5)
            .color(p.muted),
    );
    let link = ui.add(
        egui::Label::new(
            egui::RichText::new("Choose the TeX bin folder")
                .size(12.5)
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
        ui.spacing_mut().item_spacing.x = 10.0;
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
            results.add(h.try_snapshot_options(
                format!("onboarding-{scheme}"),
                &egui_kittest::SnapshotOptions::new().max_failed_pixels(
                    egui_kittest::OsThreshold::new(60).macos(2000).windows(2000),
                ),
            ));
        }
    }
}
