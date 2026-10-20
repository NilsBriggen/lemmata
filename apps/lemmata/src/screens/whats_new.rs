//! "What's new" (WhatsNew.svelte, whatsNew.ts): shown once after an update, what people can do now
//! that they couldn't before. Only releases with something worth showing have an entry.

use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind};
use eframe::egui::{self, FontId, RichText, Ui, vec2};
use std::cmp::Ordering;

pub struct Release {
    pub version: &'static str,
    pub highlights: &'static [(&'static str, &'static str)],
}

/// Newest first.
pub const RELEASES: &[Release] = &[
    Release {
        version: "2.0.0",
        highlights: &[
            (
                "Git, built in",
                "Commit, pull and push from the Git panel, switch branches, and clone a project from GitHub, GitLab or Overleaf.",
            ),
            (
                "Review every change",
                "Compare your project with an earlier snapshot and keep or undo each change, or get a PDF with the changes marked.",
            ),
            (
                "Compile without installing TeX",
                "No TeX on this computer? Lemmata can download a small engine and compile right away.",
            ),
            (
                "A template for every occasion",
                "Journal and conference papers, a full thesis, a book, a poster, slides, an exam, a cover letter and more.",
            ),
            (
                "Spell check in many more languages",
                "Besides English, German and French, you can now check Spanish, Italian, Portuguese, Dutch, Polish, Swedish, Danish, Norwegian, Czech, Russian, Ukrainian, Turkish and Greek. Pick one in Preferences and it downloads once.",
            ),
        ],
    },
    Release {
        version: "1.5.0",
        highlights: &[
            (
                "Find references online",
                "Search for papers by title and add them to your bibliography, then tidy it in the bibliography manager.",
            ),
            (
                "Hover to see what you refer to",
                "Point at a reference, citation or figure to see the equation, the paper or the image.",
            ),
            (
                "Comments and TODOs",
                "Leave comments on any part of the text and keep track of your TODOs in one list.",
            ),
            (
                "Grammar checking",
                "Turn on LanguageTool in Preferences to catch grammar mistakes as you write.",
            ),
            (
                "Clearer errors",
                "Many more TeX errors now come with a plain explanation of what went wrong and how to fix it.",
            ),
        ],
    },
];

/// Compares "1.5.2" and "2.0.0-rc.1" style versions; a pre-release comes before its release.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |v: &str| {
        let (main, pre) = match v.split_once('-') {
            Some((m, p)) => (m.to_owned(), Some(p.to_owned())),
            None => (v.to_owned(), None),
        };
        let nums: Vec<u64> = main.split('.').map(|n| n.parse().unwrap_or(0)).collect();
        (nums, pre)
    };
    let ((x, xp), (y, yp)) = (parse(a), parse(b));
    for i in 0..3 {
        let o = x.get(i).unwrap_or(&0).cmp(y.get(i).unwrap_or(&0));
        if o != Ordering::Equal {
            return o;
        }
    }
    match (xp, yp) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        // Numeric parts compare as numbers: rc.10 after rc.9.
        (Some(a), Some(b)) => {
            let key = |s: &str| {
                s.split('.')
                    .map(|p| {
                        p.parse::<u64>()
                            .map_or((1, 0, p.to_owned()), |n| (0, n, String::new()))
                    })
                    .collect::<Vec<_>>()
            };
            key(&a).cmp(&key(&b))
        }
    }
}

/// The releases to show after updating from `last` to `current`: newer than `last`, up to
/// `current`. A pre-release of 2.0.0 already shows 2.0.0's entry, and its final release doesn't
/// show it again.
pub fn releases_since(last: &str, current: &str) -> Vec<&'static Release> {
    let base = |v: &str| v.split('-').next().unwrap_or(v).to_owned();
    RELEASES
        .iter()
        .filter(|r| {
            compare_versions(r.version, &base(last)) == Ordering::Greater
                && compare_versions(r.version, &base(current)) != Ordering::Greater
        })
        .collect()
}

/// The releases to show on this start, if any. `used_before`: someone who already used the app
/// is updating to this version, even when the old one didn't remember its version.
pub fn to_show(last: &str, current: &str, used_before: bool) -> Vec<&'static Release> {
    if last == current {
        return Vec::new();
    }
    let updated = !last.is_empty() || used_before;
    let releases = if updated {
        releases_since(if last.is_empty() { "0.0.0" } else { last }, current)
    } else {
        Vec::new()
    };
    // Unknown previous version: only this release's news, not the whole history.
    if last.is_empty() {
        releases.into_iter().take(1).collect()
    } else {
        releases
    }
}

/// The dialog. Returns true when it was closed.
pub fn show(ctx: &egui::Context, p: &Palette, releases: &[&'static Release]) -> bool {
    let mut done = false;
    let subtitle = format!("You're now on version {}.", env!("CARGO_PKG_VERSION"));
    let r = widgets::modal_with(
        ctx,
        p,
        "What's new in Lemmata",
        Some(&subtitle),
        500.0,
        |ui| {
            let max_h = (ui.ctx().content_rect().height() * 0.6).min(460.0);
            let w = ui.available_width();
            ui.allocate_ui(vec2(w, max_h), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 14.0;
                        for r in releases {
                            section(ui, p, r, releases.len() > 1);
                        }
                    });
            });
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Got it")
                .kind(Kind::Primary)
                .show(ui, p)
                .clicked()
            {
                done = true;
            }
        }),
    );
    r.closed || done
}

fn section(ui: &mut Ui, p: &Palette, r: &Release, labelled: bool) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        if labelled {
            let v = r.version.strip_suffix(".0").unwrap_or(r.version);
            ui.label(
                RichText::new(format!("LEMMATA {v}"))
                    .font(FontId::new(12.0, theme::font::sans_600()))
                    .color(p.muted),
            );
        }
        for (title, text) in r.highlights {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.add_space(0.0);
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    icons::show(ui, icons::SPARKLES, 15.0, p.accent);
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new(*title)
                            .font(FontId::new(13.0, theme::font::sans_600()))
                            .color(p.ink),
                    );
                    ui.add(egui::Label::new(RichText::new(*text).color(p.muted)).wrap());
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(last: &str, current: &str) -> Vec<&'static str> {
        releases_since(last, current)
            .iter()
            .map(|r| r.version)
            .collect()
    }

    // whatsNew.test.ts
    #[test]
    fn versions_order_and_releases_show_once() {
        assert_eq!(compare_versions("1.5.2", "1.10.0"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "2.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.0.0-rc.2", "2.0.0"), Ordering::Less);
        assert_eq!(
            compare_versions("2.0.0-rc.10", "2.0.0-rc.9"),
            Ordering::Greater
        );
        assert_eq!(versions("1.4.0", "2.0.0"), ["2.0.0", "1.5.0"]);
        assert_eq!(versions("1.5.2", "2.0.0"), ["2.0.0"]);
        assert!(versions("2.0.0", "2.0.1").is_empty());
        assert!(versions("2.0.0", "2.0.0").is_empty());
        assert_eq!(versions("1.5.2", "2.0.0-rc.1"), ["2.0.0"]);
        assert!(versions("2.0.0-rc.1", "2.0.0-rc.2").is_empty());
        assert!(versions("2.0.0-rc.2", "2.0.0").is_empty());
        // A first install sees the welcome, not the news; an old user without a remembered
        // version sees only this release's.
        assert!(to_show("", "2.0.0", false).is_empty());
        assert_eq!(to_show("", "2.0.0", true).len(), 1);
    }
}
