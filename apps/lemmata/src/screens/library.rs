//! The start screen (Library.svelte), after Overleaf's project dashboard: actions on the left; on
//! the right the folders and the projects in none, as a list (or grid with thumbnails). Projects
//! are dragged into a folder, or moved with "Move to folder…" in their menu.

use crate::icons::{self, Icon};
use crate::store::{Groups, Recent, Recents, Settings, View};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, ChipTone, Kind, MenuItem, MenuResult, Size};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub enum Action {
    Open(String),
    /// Removed from the list; the files stay.
    Forget(String),
    NewProject,
    Clone,
    Import,
    OpenFolder,
    Example,
    Welcome,
    Preferences,
    Reveal(String),
    SaveTemplate(String),
    /// Move the project folder to the system trash (asks first; `State::deleting` shows progress).
    Trash(String),
    CreateMain(String),
    /// The list/grid choice changed; save settings.
    Settings,
    /// The folders changed; save them.
    Groups,
}

struct Editing {
    /// None: a new folder.
    id: Option<String>,
    name: String,
    error: Option<String>,
    /// A new folder made from a project's menu takes that project in.
    project: Option<String>,
    focused: bool,
}

pub struct Deleting {
    pub path: String,
    pub busy: bool,
    pub error: Option<String>,
}

struct Undo {
    id: String,
    name: String,
    before: Groups,
    at: Instant,
}

/// Thumbnails decoded off the UI thread: project path and image (None if it can't be read).
type Decoded = std::sync::Arc<std::sync::Mutex<Vec<(String, Option<egui::ColorImage>)>>>;
type ThumbnailRead = std::sync::Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

struct ThumbnailJob {
    path: String,
    read: ThumbnailRead,
    decoded: Decoded,
    ctx: egui::Context,
}

fn thumbnail_workers() -> &'static std::sync::mpsc::SyncSender<ThumbnailJob> {
    static WORKERS: std::sync::OnceLock<std::sync::mpsc::SyncSender<ThumbnailJob>> =
        std::sync::OnceLock::new();
    WORKERS.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<ThumbnailJob>(32);
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        for _ in 0..2 {
            let rx = rx.clone();
            std::thread::spawn(move || {
                loop {
                    let job = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
                    let Ok(job) = job else { break };
                    let img = (job.read)(&job.path).and_then(|bytes| {
                        image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                            .ok()
                            .map(|i| {
                                let i = i.to_rgba8();
                                let size = [i.width() as usize, i.height() as usize];
                                egui::ColorImage::from_rgba_unmultiplied(size, i.as_raw())
                            })
                    });
                    job.decoded
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push((job.path, img));
                    job.ctx.request_repaint();
                }
            });
        }
        tx
    })
}

#[derive(Default)]
pub struct State {
    query: String,
    view: View,
    recent_only: bool,
    pub sort_by_name: bool,
    editing: Option<Editing>,
    project_menu: Option<(String, Rect)>,
    folder_menu: Option<(String, Rect)>,
    moving: Option<String>,
    pub deleting: Option<Deleting>,
    undo: Option<Undo>,
    undo_requested: bool,
    /// "Template “x” saved…", shown for 8 s.
    pub template_note: Option<(String, Instant)>,
    /// Floating status toast (e.g. bulk Overleaf import), shown for 8 s.
    pub toast: Option<(String, Instant)>,
    /// Page 1 of each project's PDF, when there is one; None once asked and missing.
    pub thumbs: HashMap<String, Option<egui::TextureHandle>>,
    /// Thumbnails being decoded, and the decoded ones waiting for the next frame.
    pending: std::collections::HashSet<String>,
    decoded: Decoded,
}

impl State {
    /// Drops cached thumbnails and clears in-flight markers so fresh previews load next time.
    pub fn clear_thumbs(&mut self) {
        self.thumbs.clear();
        self.pending.clear();
    }

    /// New and imported projects land in the folder being looked at.
    pub fn file(&self, groups: &mut Groups, paths: &[String]) -> bool {
        let View::Group(id) = &self.view else {
            return false;
        };
        for p in paths {
            groups.move_project(p, Some(id));
        }
        true
    }
}

pub struct Ctx<'a> {
    pub recents: &'a Recents,
    pub groups: &'a mut Groups,
    pub settings: &'a mut Settings,
    pub error: Option<&'a str>,
    /// The folder that failed to open because it has no main document; offers to create one.
    pub no_main_in: Option<&'a str>,
    /// Reads a project's cached page-1 PNG on a bounded worker.
    pub thumbnail: ThumbnailRead,
    pub now_ms: f64,
}

pub fn name_of(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

fn folder_of(path: &str) -> &str {
    let t = path.trim_end_matches(['/', '\\']);
    t.rfind(['/', '\\']).map_or("", |i| &t[..i])
}

/// `/home/you/…`, `/Users/you/…` and `C:\Users\you\…` shown as `~/…`.
fn home(path: &str) -> String {
    for prefix in ["/home/", "/Users/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            return match rest.find('/') {
                Some(i) => format!("~{}", &rest[i..]),
                None => "~".into(),
            };
        }
    }
    let b = path.as_bytes();
    if b.len() > 9 && b[0].is_ascii_uppercase() && &b[1..9] == b":\\Users\\" {
        let rest = &path[9..];
        return match rest.find('\\') {
            Some(i) => format!("~{}", &rest[i..]),
            None => "~".into(),
        };
    }
    path.into()
}

/// Intl.RelativeTimeFormat(undefined, { numeric: "auto" }) in English, as the Svelte list showed it.
pub fn ago(now_ms: f64, t_ms: f64) -> String {
    if t_ms == 0.0 {
        return "—".into();
    }
    let s = (t_ms - now_ms) / 1000.0;
    for (unit, secs) in [
        ("year", 31536000.0),
        ("month", 2592000.0),
        ("week", 604800.0),
        ("day", 86400.0),
        ("hour", 3600.0),
        ("minute", 60.0),
    ] {
        if s.abs() >= secs {
            let n = (s / secs).round() as i64;
            return match (unit, n) {
                ("day", -1) => "yesterday".into(),
                ("day", 1) => "tomorrow".into(),
                (u, -1) if u != "hour" && u != "minute" => format!("last {u}"),
                (u, 1) if u != "hour" && u != "minute" => format!("next {u}"),
                (_, 0) => format!("this {unit}"),
                (u, n) if n < 0 => format!("{} {u}{} ago", -n, if n == -1 { "" } else { "s" }),
                (u, n) => format!("in {n} {u}{}", if n == 1 { "" } else { "s" }),
            };
        }
    }
    "just now".into()
}

/// Intl.Collator(undefined, { numeric: true, sensitivity: "base" }): case-blind, numbers by value.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                // Zero-allocation natural numeric run comparison.
                let read_num = |it: &mut std::iter::Peekable<std::str::Chars>| -> (usize, u128) {
                    let mut sig_len = 0usize;
                    let mut val = 0u128;
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        it.next();
                        if sig_len > 0 || c != '0' {
                            sig_len += 1;
                            val = val
                                .saturating_mul(10)
                                .saturating_add((c as u8 - b'0') as u128);
                        }
                    }
                    (sig_len, val)
                };
                let (nx, ny) = (read_num(&mut a), read_num(&mut b));
                let o = nx.cmp(&ny);
                if o.is_ne() {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o.is_ne() {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// A search looks through every folder; otherwise the view's projects. Name matches come first.
fn shown<'a>(st: &State, c: &'a Ctx) -> Vec<&'a Recent> {
    let term = st.query.trim().to_lowercase();
    let mut list: Vec<&Recent> = c
        .recents
        .0
        .iter()
        .filter(|r| {
            if !term.is_empty() {
                r.path.to_lowercase().contains(&term)
            } else if st.recent_only {
                true
            } else {
                c.groups.in_view(&r.path, &st.view)
            }
        })
        .collect();
    if !st.recent_only && (st.sort_by_name || c.settings.library_sort == "name") {
        list.sort_by(|a, b| natural_cmp(name_of(&a.path), name_of(&b.path)));
    }
    if !term.is_empty() {
        list.sort_by_key(|r| !name_of(&r.path).to_lowercase().contains(&term));
    }
    if st.recent_only && term.is_empty() {
        list.truncate(12);
    }
    list
}

struct Folder {
    id: String,
    name: String,
    count: usize,
    last: f64,
}

/// The folders, at the top level (not during a search or Recent filter), by name, with what's in them.
fn folders(st: &State, c: &Ctx) -> Vec<Folder> {
    if st.view != View::Root || st.recent_only || !st.query.trim().is_empty() {
        return Vec::new();
    }
    all_folders(c)
}

fn all_folders(c: &Ctx) -> Vec<Folder> {
    let mut out: Vec<Folder> = c
        .groups
        .groups
        .iter()
        .map(|g| {
            let inside: Vec<_> = c
                .recents
                .0
                .iter()
                .filter(|r| c.groups.members.get(&r.path) == Some(&g.id))
                .collect();
            Folder {
                id: g.id.clone(),
                name: g.name.clone(),
                count: inside.len(),
                last: inside.iter().map(|r| r.last_opened).fold(0.0, f64::max),
            }
        })
        .collect();
    out.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    out
}

fn count_text(n: usize) -> String {
    if n == 1 {
        "1 project".into()
    } else {
        format!("{n} projects")
    }
}

/// What is being dragged: a project path.
#[derive(Clone)]
struct Dragged(String);

pub fn show(ui: &mut Ui, p: &Palette, st: &mut State, c: &mut Ctx) -> Option<Action> {
    let mut action = None;
    let done = std::mem::take(&mut *st.decoded.lock().unwrap_or_else(|e| e.into_inner()));
    for (path, img) in done {
        // Not pending any more: the cache was cleared for a newer thumbnail meanwhile.
        if st.pending.remove(&path) {
            let tex = img.map(|i| {
                ui.ctx()
                    .load_texture(format!("thumb:{path}"), i, egui::TextureOptions::LINEAR)
            });
            st.thumbs.insert(path, tex);
        }
    }
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, p.app);
    let compact_nav = full.width() < 760.0;
    let main_card = if compact_nav {
        let top = Rect::from_min_size(full.min, Vec2::new(full.width(), 52.0));
        compact_bar_ui(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(top)),
            p,
            st,
            c,
            &mut action,
        );
        Rect::from_min_max(
            pos2(full.left() + 8.0, top.bottom()),
            pos2(full.right() - 8.0, full.bottom() - 8.0),
        )
    } else {
        let rail_w = if full.width() < 900.0 { 208.0 } else { 236.0 };
        let rail = Rect::from_min_size(full.min, Vec2::new(rail_w, full.height()));
        rail_ui(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(rail)),
            p,
            st,
            c,
            &mut action,
        );
        Rect::from_min_max(
            pos2(rail.right(), full.top() + 8.0),
            pos2(full.right() - 8.0, full.bottom() - 8.0),
        )
    };
    widgets::paint_card(ui, p, main_card);
    let mut m = ui.new_child(egui::UiBuilder::new().max_rect(main_card.shrink(1.0)));
    egui::ScrollArea::vertical()
        .id_salt("library")
        .show(&mut m, |ui| {
            let pad_x = if main_card.width() < 640.0 { 20 } else { 32 };
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: pad_x,
                    right: pad_x,
                    top: 24,
                    bottom: 36,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    main_ui(ui, p, st, c, &mut action);
                });
        });
    overlays(ui.ctx(), p, st, c, &mut action);
    // A dragged project follows the pointer as a small label.
    if let Some(d) = egui::DragAndDrop::payload::<Dragged>(ui.ctx())
        && let Some(pos) = ui.ctx().pointer_hover_pos()
    {
        egui::Area::new(egui::Id::unique("drag-ghost"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + Vec2::new(12.0, 8.0))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border_strong))
                    .corner_radius(theme::R_SM)
                    .shadow(p.shadow)
                    .inner_margin(egui::Margin::symmetric(9, 3))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(name_of(&d.0)).size(12.5).color(p.ink))
                    });
            });
    }
    // Notices disappear after 8 s.
    let expired = |t: &Instant| t.elapsed() > Duration::from_secs(8);
    if st.undo.as_ref().is_some_and(|u| expired(&u.at)) {
        st.undo = None;
    }
    if st.template_note.as_ref().is_some_and(|(_, t)| expired(t)) {
        st.template_note = None;
    }
    if st.toast.as_ref().is_some_and(|(_, t)| expired(t)) {
        st.toast = None;
    }
    let next_expiry = [
        st.undo.as_ref().map(|u| u.at),
        st.template_note.as_ref().map(|(_, t)| *t),
        st.toast.as_ref().map(|(_, t)| *t),
    ]
    .into_iter()
    .flatten()
    .map(|t| Duration::from_secs(8).saturating_sub(t.elapsed()))
    .min();
    if let Some(left) = next_expiry {
        ui.ctx()
            .request_repaint_after(left.max(Duration::from_millis(50)));
    }
    action
}

fn section_label(ui: &mut Ui, p: &Palette, text: &str) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: FontId::new(10.5, theme::font::sans_600()),
            color: p.faint,
            extra_letter_spacing: 0.75,
            ..Default::default()
        },
    );
    ui.label(job);
}

fn compact_bar_ui(
    ui: &mut Ui,
    p: &Palette,
    st: &mut State,
    c: &mut Ctx,
    action: &mut Option<Action>,
) {
    let rect = ui.max_rect().shrink2(Vec2::new(12.0, 0.0));
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 8.0;
    let _ = widgets::logo_mark(&mut bar, 26.0);
    bar.label(
        egui::RichText::new("Lemmata")
            .font(FontId::new(19.0, theme::font::display()))
            .color(p.ink),
    );
    bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if Button::icon(icons::SETTINGS, "Preferences")
            .kind(Kind::Ghost)
            .size(Size::Sm)
            .show_icon(ui, p)
            .clicked()
        {
            *action = Some(Action::Preferences);
        }
        if Button::icon(icons::FOLDER_PLUS, "New folder")
            .kind(Kind::Ghost)
            .size(Size::Sm)
            .show_icon(ui, p)
            .clicked()
        {
            start_edit(st, c, None, None);
        }
        if Button::new("New project")
            .with_icon(icons::PLUS)
            .kind(Kind::Primary)
            .size(Size::Sm)
            .show(ui, p)
            .clicked()
        {
            *action = Some(Action::NewProject);
        }
    });
}

fn rail_ui(ui: &mut Ui, p: &Palette, st: &mut State, c: &mut Ctx, action: &mut Option<Action>) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.app);
    let inner = rect
        .shrink2(Vec2::new(12.0, 0.0))
        .with_min_y(rect.top() + 16.0)
        .with_max_y(rect.bottom() - 14.0);
    let foot_h = 68.0;
    let body_rect = Rect::from_min_max(inner.min, pos2(inner.right(), inner.bottom() - foot_h));
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
    ui.spacing_mut().item_spacing.y = 6.0;

    // Brand mark + Young Serif wordmark on quiet stone ground.
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing.x = 9.0;
        let _ = widgets::logo_mark(ui, 26.0);
        ui.label(
            egui::RichText::new("Lemmata")
                .font(FontId::new(21.0, theme::font::display()))
                .color(p.ink),
        );
    });
    ui.add_space(8.0);

    let w = ui.available_width();
    if Button::new("New project")
        .with_icon(icons::PLUS)
        .kind(Kind::Primary)
        .size(Size::Lg)
        .min_width(w)
        .show(&mut ui, p)
        .clicked()
    {
        *action = Some(Action::NewProject);
    }
    ui.add_space(6.0);

    // Primary library navigation.
    let scroll_rect = Rect::from_min_max(pos2(body_rect.left(), ui.cursor().top()), body_rect.max);
    let mut sc = ui.new_child(egui::UiBuilder::new().max_rect(scroll_rect));
    egui::ScrollArea::vertical()
        .id_salt("library-rail")
        .auto_shrink([false, false])
        .show(&mut sc, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;

            let total_n = c.recents.0.len();
            let total_s = (total_n > 0).then(|| total_n.to_string());
            if widgets::side_row(
                ui,
                p,
                Some(icons::LAYOUT_GRID),
                None,
                "All projects",
                st.view == View::Root && !st.recent_only,
                total_s.as_deref(),
            )
            .clicked()
            {
                st.view = View::Root;
                st.recent_only = false;
            }
            if widgets::side_row(
                ui,
                p,
                Some(icons::HISTORY),
                None,
                "Recent",
                st.view == View::Root && st.recent_only,
                None,
            )
            .clicked()
            {
                st.view = View::Root;
                st.recent_only = true;
            }
            if widgets::side_row(
                ui,
                p,
                Some(icons::LAYOUT_TEMPLATE),
                None,
                "Templates",
                false,
                None,
            )
            .clicked()
            {
                *action = Some(Action::NewProject);
            }

            // Folders section with inline '+' button (single "New folder" trigger).
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                section_label(ui, p, "Folders");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if Button::icon(icons::FOLDER_PLUS, "New folder")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .icon_size(14.0)
                        .show_icon(ui, p)
                        .on_hover_text("New folder")
                        .clicked()
                    {
                        start_edit(st, c, None, None);
                    }
                });
            });
            let side_folders = all_folders(c);
            if side_folders.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Organize projects into folders")
                            .size(11.5)
                            .color(p.faint),
                    );
                });
            } else {
                for f in &side_folders {
                    let on = st.view == View::Group(f.id.clone());
                    let cnt = f.count.to_string();
                    let r = widgets::side_row(
                        ui,
                        p,
                        Some(if on {
                            icons::FOLDER_OPEN
                        } else {
                            icons::FOLDER
                        }),
                        Some(p.warn),
                        &f.name,
                        on,
                        Some(cnt.as_str()),
                    );
                    if egui::DragAndDrop::has_payload_of_type::<Dragged>(ui.ctx())
                        && r.contains_pointer()
                    {
                        ui.painter().rect_stroke(
                            r.rect,
                            CornerRadius::same(7),
                            Stroke::new(1.5, p.accent),
                            StrokeKind::Inside,
                        );
                    }
                    if let Some(d) = r.dnd_release_payload::<Dragged>() {
                        c.groups.move_project(&d.0, Some(&f.id));
                        commit(st, action);
                    }
                    if r.secondary_clicked() {
                        st.folder_menu = Some((f.id.clone(), r.rect));
                    } else if r.clicked() {
                        st.view = View::Group(f.id.clone());
                        st.recent_only = false;
                    }
                }
            }

            // Bring in a project section.
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                section_label(ui, p, "Bring in a project");
            });
            for (icon, label, act) in [
                (icons::FOLDER_OPEN, "Open folder…", Action::OpenFolder),
                (icons::GIT_BRANCH, "Clone a repository…", Action::Clone),
                (icons::FILE_ARCHIVE, "Import from Overleaf…", Action::Import),
                (icons::SPARKLES, "Example document", Action::Example),
            ] {
                if widgets::side_row(ui, p, Some(icon), None, label, false, None).clicked() {
                    *action = Some(act);
                }
            }

            let note_font = FontId::proportional(12.0);
            if let Some((note, _)) = &st.template_note {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    widgets::paragraph(
                        ui,
                        note,
                        note_font.clone(),
                        p.muted,
                        1.45,
                        ui.available_width() - 6.0,
                    );
                });
            }
            if let Some(u) = &st.undo {
                ui.add_space(8.0);
                let mut undo = false;
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(6.0);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(
                        egui::RichText::new(format!("Folder “{}” deleted · ", u.name))
                            .font(note_font.clone())
                            .color(p.muted),
                    );
                    undo = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new("Undo")
                                    .font(note_font.clone())
                                    .color(p.accent_text)
                                    .underline(),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked();
                });
                if undo {
                    st.undo_requested = true;
                }
            }
        });

    // Bottom pinned utilities on quiet ground.
    let foot = Rect::from_min_max(pos2(inner.left(), inner.bottom() - foot_h + 6.0), inner.max);
    ui.painter()
        .hline(foot.x_range(), foot.top() - 4.0, Stroke::new(1.0, p.border));
    let mut f = ui.new_child(egui::UiBuilder::new().max_rect(foot));
    f.spacing_mut().item_spacing.y = 2.0;
    if widgets::side_row(
        &mut f,
        p,
        Some(icons::SETTINGS),
        None,
        "Preferences",
        false,
        None,
    )
    .clicked()
    {
        *action = Some(Action::Preferences);
    }
    if widgets::side_row(
        &mut f,
        p,
        Some(icons::SPARKLES),
        None,
        "Show welcome screen",
        false,
        None,
    )
    .clicked()
    {
        *action = Some(Action::Welcome);
    }
}

fn main_ui(ui: &mut Ui, p: &Palette, st: &mut State, c: &mut Ctx, action: &mut Option<Action>) {
    let searching = !st.query.trim().is_empty();
    let active = match &st.view {
        View::Group(id) => c.groups.groups.iter().find(|g| &g.id == id).cloned(),
        View::Root => None,
    };
    // Header: single row when wide enough, stacked above a compact tools row when narrow.
    st.sort_by_name = c.settings.library_sort == "name";
    let h1 = FontId::new(26.0, theme::font::display());
    let narrow_header = ui.available_width() < 760.0;
    let draw_crumb = |ui: &mut Ui, st: &mut State, c: &mut Ctx, action: &mut Option<Action>| {
        match (&active, searching) {
            (Some(g), false) => {
                let (pos, galley, crumb) =
                    egui::Label::new(egui::RichText::new("Your projects").font(h1.clone()))
                        .selectable(false)
                        .sense(Sense::click())
                        .layout_in_ui(ui);
                let crumb = crumb.on_hover_cursor(egui::CursorIcon::PointingHand);
                crumb.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Link, true, "Your projects")
                });
                // Also a drop target: dropping a project here takes it out of its folder.
                let over = crumb.dnd_hover_payload::<Dragged>().is_some();
                if over {
                    ui.painter().rect(
                        crumb.rect.expand(4.0),
                        theme::R,
                        p.accent_soft,
                        Stroke::new(1.5, p.accent),
                        StrokeKind::Outside,
                    );
                }
                let color = if over || crumb.hovered() {
                    p.accent
                } else {
                    p.muted
                };
                ui.painter().galley(pos, galley, color);
                if let Some(d) = crumb.dnd_release_payload::<Dragged>() {
                    c.groups.move_project(&d.0, None);
                    commit(st, action);
                }
                if crumb.clicked() {
                    st.view = View::Root;
                }
                icons::show(ui, icons::CHEVRON_RIGHT, 20.0, p.faint);
                ui.add(
                    egui::Label::new(egui::RichText::new(&g.name).font(h1.clone()).color(p.ink))
                        .truncate(),
                );
            }
            _ => {
                ui.label(
                    egui::RichText::new("Your projects")
                        .font(h1.clone())
                        .color(p.ink),
                );
            }
        }
    };
    if narrow_header {
        ui.horizontal(|ui| {
            draw_crumb(ui, st, c, action);
        });
        ui.add_space(10.0);
        let row_w = ui.available_width();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            let search_w = (row_w - 310.0).clamp(140.0, 220.0);
            ui.allocate_ui(Vec2::new(search_w, 32.0), |ui| {
                widgets::search_input(ui, p, &mut st.query, "Search projects");
            });
            let grid = c.settings.library_view == "grid";
            if let Some(i) = widgets::segmented(
                ui,
                p,
                &[
                    (Some(icons::LIST), "List"),
                    (Some(icons::LAYOUT_GRID), "Grid"),
                ],
                grid as usize,
                true,
            ) {
                c.settings.library_view = ["list", "grid"][i].into();
                *action = Some(Action::Settings);
            }
            let mut value = if c.settings.library_sort == "name" {
                "name".to_owned()
            } else {
                "opened".to_owned()
            };
            ui.allocate_ui(Vec2::new(114.0, 32.0), |ui| {
                if widgets::select(
                    ui,
                    p,
                    "Sort projects",
                    &mut value,
                    &[("opened", "Last opened"), ("name", "Name")],
                ) {
                    st.sort_by_name = value == "name";
                    c.settings.library_sort = value;
                    *action = Some(Action::Settings);
                }
            });
            if let (Some(g), false) = (&active, searching) {
                if Button::new("Rename")
                    .with_icon(icons::SQUARE_PEN)
                    .size(Size::Sm)
                    .show(ui, p)
                    .clicked()
                {
                    start_edit(st, c, Some(g.id.clone()), None);
                }
                if Button::new("Delete folder")
                    .with_icon(icons::TRASH)
                    .size(Size::Sm)
                    .show(ui, p)
                    .clicked()
                {
                    remove_group(st, c, &g.id, action);
                }
            }
        });
    } else {
        ui.horizontal(|ui| {
            // The tools now come first, so centre them on the title's height as before.
            let title_h = ui.fonts_mut(|f| {
                f.layout_no_wrap("Your projects".into(), h1.clone(), Color32::PLACEHOLDER)
                    .size()
                    .y
            });
            ui.set_min_height(title_h.max(36.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let gap = ui.spacing().item_spacing.x;
                ui.spacing_mut().item_spacing.x = 8.0;
                let mut value = if c.settings.library_sort == "name" {
                    "name".to_owned()
                } else {
                    "opened".to_owned()
                };
                ui.allocate_ui(Vec2::new(114.0, 32.0), |ui| {
                    if widgets::select(
                        ui,
                        p,
                        "Sort projects",
                        &mut value,
                        &[("opened", "Last opened"), ("name", "Name")],
                    ) {
                        st.sort_by_name = value == "name";
                        c.settings.library_sort = value;
                        *action = Some(Action::Settings);
                    }
                });
                let grid = c.settings.library_view == "grid";
                if let Some(i) = widgets::segmented(
                    ui,
                    p,
                    &[
                        (Some(icons::LIST), "List"),
                        (Some(icons::LAYOUT_GRID), "Grid"),
                    ],
                    grid as usize,
                    true,
                ) {
                    c.settings.library_view = ["list", "grid"][i].into();
                    *action = Some(Action::Settings);
                }
                ui.allocate_ui(Vec2::new(260.0, 32.0), |ui| {
                    widgets::search_input(ui, p, &mut st.query, "Search projects");
                });
                if let (Some(g), false) = (&active, searching) {
                    if Button::new("Delete folder")
                        .with_icon(icons::TRASH)
                        .size(Size::Sm)
                        .show(ui, p)
                        .clicked()
                    {
                        remove_group(st, c, &g.id, action);
                    }
                    if Button::new("Rename")
                        .with_icon(icons::SQUARE_PEN)
                        .size(Size::Sm)
                        .show(ui, p)
                        .clicked()
                    {
                        start_edit(st, c, Some(g.id.clone()), None);
                    }
                }
                // Tools first, so the breadcrumb gets what is left and truncates instead of running under them.
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    draw_crumb(ui, st, c, action);
                });
            });
        });
    }
    ui.add_space(18.0);

    if let Some(err) = c.error {
        egui::Frame::new().fill(p.error_soft).corner_radius(theme::R).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let text = if c.no_main_in.is_some() { "This folder has no main document: no .tex file in it starts with \\documentclass." } else { err };
                ui.label(egui::RichText::new(text).color(p.error));
                if let Some(dir) = c.no_main_in
                    && Button::new("Create main.tex and open").size(Size::Sm).show(ui, p).clicked()
                {
                    *action = Some(Action::CreateMain(dir.into()));
                }
            });
        });
        ui.add_space(14.0);
    }

    let folders = folders(st, c);
    let shown: Vec<Recent> = shown(st, c).into_iter().cloned().collect();
    if c.recents.0.is_empty() && folders.is_empty() && active.is_none() {
        empty(ui, p, action);
    } else if folders.is_empty() && shown.is_empty() {
        if searching {
            ui.label(
                egui::RichText::new(format!("No project matches “{}”.", st.query)).color(p.muted),
            );
        } else if active.is_some() {
            widgets::paragraph(
                ui,
                "No projects in this folder yet. Drag one onto the folder in Your projects, use Move to folder… in its menu, or create a new project here.",
                FontId::proportional(13.0),
                p.muted,
                1.45,
                ui.available_width(),
            );
        }
    } else if c.settings.library_view == "grid" {
        grid(ui, p, st, c, &folders, &shown, action);
    } else {
        // Hero "Continue writing" cards above the table on the root view when not searching.
        if active.is_none() && !searching && !st.recent_only && !c.recents.0.is_empty() {
            continue_writing_row(ui, p, st, c, action);
            ui.add_space(22.0);
            section_label(ui, p, "All projects");
            ui.add_space(8.0);
        }
        table(ui, p, st, c, &folders, &shown, action);
    }
}

/// Up to 3 recent-project hero cards in list view ("Continue writing").
fn continue_writing_row(
    ui: &mut Ui,
    p: &Palette,
    st: &mut State,
    c: &mut Ctx,
    action: &mut Option<Action>,
) {
    let top: Vec<Recent> = c.recents.0.iter().take(3).cloned().collect();
    if top.is_empty() {
        return;
    }
    section_label(ui, p, "Continue writing");
    ui.add_space(10.0);
    let w = ui.available_width();
    let grid_cols = if w < 560.0 {
        1
    } else if w < 820.0 {
        2
    } else {
        3
    };
    let cols = grid_cols.min(top.len());
    let gap = 14.0;
    let cw = if top.len() == 1 && grid_cols > 1 {
        ((w - gap * (grid_cols as f32 - 1.0)) / grid_cols as f32 * 1.45).min(w)
    } else {
        (w - gap * (cols as f32 - 1.0)) / cols as f32
    };
    let ch = 134.0;
    let (area, _) = ui.allocate_exact_size(Vec2::new(w, ch), Sense::hover());
    let painter = ui.painter().clone();
    for (i, rec) in top.iter().take(cols).enumerate() {
        let rect = Rect::from_min_size(
            pos2(area.left() + i as f32 * (cw + gap), area.top()),
            Vec2::new(cw, ch),
        );
        let r = ui
            .interact(
                rect,
                ui.make_persistent_id(("hero-card", &rec.path)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let pname = name_of(&rec.path);
        r.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                true,
                format!("Continue writing: {pname}"),
            )
        });
        let hovered = r.hovered();
        let rad = CornerRadius::same(theme::R_LG);
        if hovered {
            painter.add(p.shadow.as_shape(rect, rad));
        } else {
            painter.add(p.shadow_card.as_shape(rect, rad));
        }
        painter.rect(
            rect,
            rad,
            p.surface,
            Stroke::new(1.0, if hovered { p.border_strong } else { p.border }),
            StrokeKind::Inside,
        );
        // Left miniature page thumbnail (76 x 106).
        let thumb_r = Rect::from_min_size(rect.min + Vec2::new(12.0, 14.0), Vec2::new(76.0, 106.0));
        painter.rect(
            thumb_r,
            CornerRadius::same(theme::R_SM),
            p.surface_2,
            Stroke::new(1.0, p.border),
            StrokeKind::Inside,
        );
        if ui.is_rect_visible(thumb_r) {
            match thumb(ui, st, c, &rec.path) {
                Some(tex) => {
                    let [tw, th] = tex.size().map(|v| v as f32);
                    let inner = thumb_r.shrink(1.0);
                    let uv_h = ((inner.height() / inner.width()) * tw / th).min(1.0);
                    painter.rect_filled(inner, CornerRadius::same(3), Color32::WHITE);
                    painter.image(
                        tex.id(),
                        inner,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, uv_h)),
                        Color32::WHITE,
                    );
                }
                None => {
                    let inner = thumb_r.shrink(1.0);
                    let page_bg = if p.dark { p.surface } else { Color32::WHITE };
                    let title_bar = if p.dark {
                        p.faint
                    } else {
                        Color32::from_rgb(0xa6, 0xae, 0xb6)
                    };
                    let line_bar = if p.dark {
                        p.border_strong
                    } else {
                        Color32::from_rgb(0xdf, 0xe3, 0xe7)
                    };
                    painter.rect_filled(inner, CornerRadius::same(3), page_bg);
                    let mut ly = inner.top() + 14.0;
                    painter.rect_filled(
                        Rect::from_min_size(
                            pos2(inner.left() + 12.0, ly),
                            Vec2::new(inner.width() - 28.0, 5.0),
                        ),
                        2,
                        title_bar,
                    );
                    ly += 13.0;
                    for k in 0..5 {
                        let lw = inner.width() - 20.0 - if k == 4 { 16.0 } else { 0.0 };
                        painter.rect_filled(
                            Rect::from_min_size(
                                pos2(inner.left() + 10.0, ly),
                                Vec2::new(lw.max(12.0), 3.0),
                            ),
                            1,
                            line_bar,
                        );
                        ly += 9.0;
                    }
                }
            }
        }
        // Right metadata column.
        let tx = thumb_r.right() + 14.0;
        let tw = (rect.right() - tx - 14.0).max(40.0);
        let tg = one_line(
            &painter,
            pname,
            FontId::new(14.0, theme::font::sans_600()),
            if hovered { p.accent_text } else { p.ink },
            tw,
        );
        painter.galley(pos2(tx, rect.top() + 18.0), tg, p.ink);
        let loc = one_line(
            &painter,
            &home(folder_of(&rec.path)),
            FontId::monospace(11.0),
            p.faint,
            tw,
        );
        painter.galley(pos2(tx, rect.top() + 40.0), loc, p.faint);
        let when = one_line(
            &painter,
            &ago(c.now_ms, rec.last_opened),
            FontId::proportional(12.0),
            p.muted,
            tw,
        );
        painter.galley(pos2(tx, rect.top() + 62.0), when, p.muted);
        if let Some(fname) = folder_name(c.groups, &rec.path) {
            let mut chip_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                pos2(tx, rect.bottom() - 34.0),
                Vec2::new(tw, 22.0),
            )));
            widgets::chip(
                &mut chip_ui,
                p,
                Some(icons::FOLDER),
                &fname,
                ChipTone::Neutral,
            );
        }
        if r.clicked() {
            *action = Some(Action::Open(rec.path.clone()));
        }
    }
}

fn empty(ui: &mut Ui, p: &Palette, action: &mut Option<Action>) {
    ui.add_space(0.1 * ui.ctx().content_rect().height());
    widgets::column(ui, 460.0, |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            icons::show(ui, icons::FILE_TEXT, 36.0, p.faint);
            ui.add_space(6.0);
            ui.label(egui::RichText::new("No projects yet").font(FontId::new(22.0, theme::font::display())).color(p.ink));
            let text = "Start a new one from a template, open a folder you already have, or bring your projects over from Overleaf (select them in Overleaf, then Download).";
            let mut job = widgets::Rich::new().add(text, FontId::proportional(13.0), p.muted).line_height(13.0 * 1.6).wrap(460.0).0;
            job.halign = egui::Align::Center;
            ui.add(egui::Label::new(job));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let w = 140.0 + 8.0 + 190.0;
                ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
                if Button::new("New project").with_icon(icons::PLUS).kind(Kind::Primary).show(ui, p).clicked() {
                    *action = Some(Action::NewProject);
                }
                if Button::new("Import from Overleaf").with_icon(icons::FILE_ARCHIVE).show(ui, p).clicked() {
                    *action = Some(Action::Import);
                }
            });
        });
    });
}

/// Puts the ⋯ button at the end of a row and opens the menu on click or right-click.
fn more_button(
    ui: &mut Ui,
    p: &Palette,
    rect: Rect,
    label: &str,
    row_hovered: bool,
    ghost: bool,
) -> (bool, Rect) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    // .acts: faint until the row is hovered.
    child.set_opacity(if row_hovered { 1.0 } else { 0.45 });
    let b = Button::icon(icons::ELLIPSIS, label)
        .size(Size::Sm)
        .kind(if ghost { Kind::Ghost } else { Kind::Default })
        .show_icon(&mut child, p)
        .on_hover_text(label);
    (b.clicked(), b.rect)
}

fn table(
    ui: &mut Ui,
    p: &Palette,
    st: &mut State,
    c: &mut Ctx,
    folders: &[Folder],
    shown: &[Recent],
    action: &mut Option<Action>,
) {
    let w = ui.available_width();
    // grid-template-columns: minmax(200px, 2fr) minmax(160px, 2fr) 130px 48px
    let cols = if w < 580.0 {
        let rem = (w - 2.0 - 178.0).max(200.0);
        let c0 = (rem * 0.54).max(120.0);
        let c1 = (rem - c0).max(80.0);
        [c0, c1, 130.0, 48.0]
    } else {
        let fr = ((w - 2.0 - 178.0) / 2.0).max(200.0);
        [fr.max(200.0), fr.max(160.0), 130.0, 48.0]
    };
    let x = |i: usize| cols[..i].iter().sum::<f32>();
    let rows = 1 + folders.len() + shown.len();
    let height = 34.0 + (rows - 1) as f32 * 53.0 + 2.0;
    let (outer, _) = ui.allocate_exact_size(Vec2::new(w, height), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect(
        outer,
        CornerRadius::same(theme::R_LG),
        p.surface,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    let inner = outer.shrink(1.0);
    // Header row on --surface-2.
    let th = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 34.0));
    painter.rect_filled(
        th,
        CornerRadius {
            nw: theme::R_LG - 1,
            ne: theme::R_LG - 1,
            sw: 0,
            se: 0,
        },
        p.surface_2,
    );
    for (i, h) in ["NAME", "LOCATION", "LAST OPENED"].iter().enumerate() {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            h,
            0.0,
            egui::TextFormat {
                font_id: FontId::new(11.0, theme::font::sans_600()),
                color: p.muted,
                extra_letter_spacing: 0.66,
                ..Default::default()
            },
        );
        let g = ui.fonts_mut(|f| f.layout_job(job));
        painter.galley(
            pos2(th.left() + x(i) + 14.0, th.center().y - g.size().y / 2.0),
            g,
            p.muted,
        );
    }
    let mut y = th.bottom();
    let row_rect =
        |y: f32| Rect::from_min_size(pos2(inner.left(), y), Vec2::new(inner.width(), 53.0));
    for f in folders {
        let rect = row_rect(y);
        let id = ui.make_persistent_id(("folder-row", &f.id));
        let r = ui.interact(rect, id, Sense::click());
        let dragging_over =
            egui::DragAndDrop::has_payload_of_type::<Dragged>(ui.ctx()) && r.contains_pointer();
        row_background(&painter, p, rect, r.hovered(), dragging_over);
        let name_r = Rect::from_min_size(
            pos2(rect.left(), rect.top() + 1.0),
            Vec2::new(cols[0], 52.0),
        );
        icons::paint(
            &painter,
            Rect::from_center_size(
                pos2(name_r.left() + 14.0 + 15.0, name_r.center().y),
                Vec2::splat(18.0),
            ),
            icons::FOLDER,
            p.accent,
        );
        let name_hover = r.hover_pos().is_some_and(|pos| name_r.contains(pos));
        row_name(&painter, p, name_r, &f.name, name_hover, None);
        painter.text(
            pos2(rect.left() + x(1) + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            count_text(f.count),
            FontId::proportional(12.0),
            p.muted,
        );
        let when = if f.count > 0 {
            ago(c.now_ms, f.last)
        } else {
            "—".into()
        };
        painter.text(
            pos2(rect.left() + x(2) + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            when,
            FontId::proportional(13.0),
            p.muted,
        );
        let acts = Rect::from_min_size(
            pos2(
                rect.left() + x(3) + 48.0 - 8.0 - 26.0,
                rect.center().y - 13.0,
            ),
            Vec2::splat(26.0),
        );
        let (clicked, anchor) = more_button(
            ui,
            p,
            acts,
            &format!("Actions for folder {}", f.name),
            r.hovered(),
            true,
        );
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Row, true, &f.name));
        if clicked || r.secondary_clicked() {
            st.folder_menu = Some((f.id.clone(), anchor));
        } else if r.clicked() {
            st.view = View::Group(f.id.clone());
        }
        if let Some(d) = r.dnd_release_payload::<Dragged>() {
            c.groups.move_project(&d.0, Some(&f.id));
            commit(st, action);
        }
        y += 53.0;
    }
    for rec in shown {
        let rect = row_rect(y);
        let id = ui.make_persistent_id(("project-row", &rec.path));
        let r = ui.interact(rect, id, Sense::click_and_drag());
        r.dnd_set_drag_payload(Dragged(rec.path.clone()));
        let dragged = r.dragged();
        if dragged {
            painter.rect_filled(rect, 0, p.surface.gamma_multiply(0.5));
        }
        row_background(&painter, p, rect, r.hovered() && !dragged, false);
        let name_r = Rect::from_min_size(
            pos2(rect.left(), rect.top() + 1.0),
            Vec2::new(cols[0], 52.0),
        );
        let mini = Rect::from_center_size(
            pos2(name_r.left() + 14.0 + 15.0, name_r.center().y),
            Vec2::new(30.0, 38.0),
        );
        thumb_mini(ui, &painter, p, st, c, &rec.path, mini);
        let chip = (!st.query.trim().is_empty())
            .then(|| folder_name(c.groups, &rec.path))
            .flatten();
        let name_hover = r.hover_pos().is_some_and(|pos| name_r.contains(pos));
        row_name(
            &painter,
            p,
            name_r,
            name_of(&rec.path),
            name_hover,
            chip.as_deref(),
        );
        let loc = home(folder_of(&rec.path));
        let loc_w = (cols[1] - 28.0).max(24.0);
        let loc_rect = Rect::from_min_size(
            pos2(rect.left() + x(1) + 14.0, rect.top()),
            Vec2::new(loc_w, 53.0),
        );
        let mut loc_job =
            egui::text::LayoutJob::simple_singleline(loc, FontId::monospace(12.0), p.muted);
        loc_job.wrap.max_width = loc_w;
        loc_job.wrap.max_rows = 1;
        loc_job.wrap.overflow_character = Some('…');
        let loc_g = ui.fonts_mut(|f| f.layout_job(loc_job));
        painter.galley(
            pos2(loc_rect.left(), rect.center().y - loc_g.size().y / 2.0),
            loc_g,
            p.muted,
        );
        painter.text(
            pos2(rect.left() + x(2) + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            ago(c.now_ms, rec.last_opened),
            FontId::proportional(13.0),
            p.muted,
        );
        let acts = Rect::from_min_size(
            pos2(
                rect.left() + x(3) + 48.0 - 8.0 - 26.0,
                rect.center().y - 13.0,
            ),
            Vec2::splat(26.0),
        );
        let (clicked, anchor) = more_button(
            ui,
            p,
            acts,
            &format!("Actions for {}", name_of(&rec.path)),
            r.hovered(),
            true,
        );
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Row, true, name_of(&rec.path)));
        if clicked || r.secondary_clicked() {
            st.project_menu = Some((rec.path.clone(), anchor));
        } else if r.clicked() {
            *action = Some(Action::Open(rec.path.clone()));
        }
        y += 53.0;
    }
    // Row rules.
    let mut y = th.bottom();
    for _ in 0..rows - 1 {
        painter.hline(inner.x_range(), y, Stroke::new(1.0, p.border));
        y += 53.0;
    }
}

fn row_background(painter: &egui::Painter, p: &Palette, rect: Rect, hovered: bool, drop: bool) {
    if drop {
        painter.rect(
            rect.shrink(1.0),
            0,
            p.accent_soft,
            Stroke::new(2.0, p.accent),
            StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_filled(rect, 0, p.surface_2);
    }
}

/// The project or folder name, accent and underlined on hover, with a folder chip during a search.
fn row_name(
    painter: &egui::Painter,
    p: &Palette,
    rect: Rect,
    name: &str,
    hover: bool,
    chip: Option<&str>,
) {
    let x = rect.left() + 14.0 + 30.0 + 12.0;
    let font = FontId::new(13.0, theme::font::sans_500());
    let color = if hover { p.accent } else { p.ink };
    let chip = chip.map(|c| painter.layout_no_wrap(c.into(), FontId::proportional(11.0), p.muted));
    // The name stops at the end of its column, before the chip's room (or the location's).
    let room = rect.right() - x - 8.0 - chip.as_ref().map_or(0.0, |g| g.size().x + 22.0);
    let galley = one_line(painter, name, font, color, room);
    let pos = pos2(x, rect.center().y - galley.size().y / 2.0);
    let w = galley.size().x;
    if hover {
        painter.hline(
            x..=x + w,
            pos.y + galley.size().y - 1.0,
            Stroke::new(1.0, color),
        );
    }
    painter.galley(pos, galley, color);
    if let Some(g) = chip {
        let r = Rect::from_min_size(
            pos2(x + w + 8.0, rect.center().y - 9.0),
            Vec2::new(g.size().x + 14.0, 18.0),
        );
        painter.rect_filled(r, CornerRadius::same(9), p.surface_3);
        painter.galley(
            pos2(r.left() + 7.0, r.center().y - g.size().y / 2.0),
            g,
            p.muted,
        );
    }
}

fn folder_name(groups: &Groups, path: &str) -> Option<String> {
    let id = groups.members.get(path)?;
    groups
        .groups
        .iter()
        .find(|g| &g.id == id)
        .map(|g| g.name.clone())
}

/// The thumbnail if it is ready. Decoding takes a few ms per project, so it runs off the UI
/// thread and the library shows at once, the pages filling in a frame later.
fn thumb(ui: &Ui, st: &mut State, c: &Ctx, path: &str) -> Option<egui::TextureHandle> {
    if let Some(t) = st.thumbs.get(path) {
        return t.clone();
    }
    if !st.pending.insert(path.to_owned()) {
        return None;
    }
    let job = ThumbnailJob {
        path: path.to_owned(),
        read: c.thumbnail.clone(),
        decoded: st.decoded.clone(),
        ctx: ui.ctx().clone(),
    };
    if thumbnail_workers().try_send(job).is_err() {
        st.pending.remove(path);
    }
    None
}

/// `.mini`: a 30×38 page thumbnail, or a file icon until there is one.
fn thumb_mini(
    ui: &Ui,
    painter: &egui::Painter,
    p: &Palette,
    st: &mut State,
    c: &Ctx,
    path: &str,
    rect: Rect,
) {
    painter.rect(
        rect,
        CornerRadius::same(3),
        p.surface_3,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    // Rows scrolled out of view don't load theirs.
    if !ui.is_rect_visible(rect) {
        return;
    }
    match thumb(ui, st, c, path) {
        Some(tex) => {
            // object-fit: cover; object-position: top
            let [w, h] = tex.size().map(|v| v as f32);
            let inner = rect.shrink(1.0);
            let visible_h = (inner.height() / inner.width()) * w / h;
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, visible_h.min(1.0)));
            painter.image(tex.id(), inner, uv, Color32::WHITE);
        }
        None => icons::paint(
            painter,
            Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
            icons::FILE_TEXT,
            p.faint,
        ),
    }
}

fn grid(
    ui: &mut Ui,
    p: &Palette,
    st: &mut State,
    c: &mut Ctx,
    folders: &[Folder],
    shown: &[Recent],
    action: &mut Option<Action>,
) {
    // grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 18px
    let w = ui.available_width();
    let cols = (((w + 18.0) / 198.0).floor() as usize).max(1);
    let cw = (w - 18.0 * (cols as f32 - 1.0)) / cols as f32;
    let ch = 2.0 + 210.0 + 10.0 + 18.0 + 1.0 + 16.0 + 10.0;
    let n = folders.len() + shown.len();
    let rows = n.div_ceil(cols);
    let (area, _) = ui.allocate_exact_size(
        Vec2::new(w, rows as f32 * ch + rows.saturating_sub(1) as f32 * 18.0),
        Sense::hover(),
    );
    let painter = ui.painter().clone();
    let cell = |i: usize| {
        Rect::from_min_size(
            area.min
                + Vec2::new(
                    (i % cols) as f32 * (cw + 18.0),
                    (i / cols) as f32 * (ch + 18.0),
                ),
            Vec2::new(cw, ch),
        )
    };
    for (i, f) in folders.iter().enumerate() {
        let rect = cell(i);
        let r = ui.interact(
            rect,
            ui.make_persistent_id(("folder-card", &f.id)),
            Sense::click(),
        );
        let drop =
            egui::DragAndDrop::has_payload_of_type::<Dragged>(ui.ctx()) && r.contains_pointer();
        let lift = card_frame(
            ui,
            &painter,
            p,
            rect,
            r.hovered(),
            drop,
            |painter, thumb| {
                painter.rect_filled(thumb, 0, p.surface_2);
                icons::paint(
                    painter,
                    Rect::from_center_size(thumb.center(), Vec2::splat(56.0)),
                    icons::FOLDER,
                    p.accent,
                );
            },
        );
        card_info(&painter, p, lift, &f.name, &count_text(f.count));
        let (clicked, anchor) = more_button(
            ui,
            p,
            Rect::from_min_size(
                pos2(lift.right() - 8.0 - 26.0, lift.top() + 8.0),
                Vec2::splat(26.0),
            ),
            &format!("Actions for folder {}", f.name),
            r.hovered(),
            false,
        );
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &f.name));
        if clicked || r.secondary_clicked() {
            st.folder_menu = Some((f.id.clone(), anchor));
        } else if r.clicked() {
            st.view = View::Group(f.id.clone());
        }
        if let Some(d) = r.dnd_release_payload::<Dragged>() {
            c.groups.move_project(&d.0, Some(&f.id));
            commit(st, action);
        }
    }
    for (j, rec) in shown.iter().enumerate() {
        let rect = cell(folders.len() + j);
        let r = ui.interact(
            rect,
            ui.make_persistent_id(("project-card", &rec.path)),
            Sense::click_and_drag(),
        );
        r.dnd_set_drag_payload(Dragged(rec.path.clone()));
        let tex = if ui.is_rect_visible(rect) {
            thumb(ui, st, c, &rec.path)
        } else {
            None
        };
        let mut lift = card_frame(
            ui,
            &painter,
            p,
            rect,
            r.hovered() && !r.dragged(),
            false,
            |painter, thumb| {
                painter.rect_filled(thumb, 0, p.surface_3);
                // The page, 76 % wide from 16 px down, or a sketch of one.
                let page_w = thumb.width() * if tex.is_some() { 0.76 } else { 0.70 };
                let page = Rect::from_min_max(
                    pos2(thumb.center().x - page_w / 2.0, thumb.top() + 16.0),
                    pos2(thumb.center().x + page_w / 2.0, thumb.bottom()),
                );
                painter.add(p.shadow_card.as_shape(page, 0));
                match &tex {
                    Some(t) => {
                        let [tw, th] = t.size().map(|v| v as f32);
                        let full_h = page.width() * th / tw;
                        let uv_h = (page.height() / full_h).min(1.0);
                        let page = page.with_max_y(page.top() + full_h.min(page.height()));
                        painter.rect_filled(page, 0, Color32::WHITE);
                        painter.image(
                            t.id(),
                            page,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, uv_h)),
                            Color32::WHITE,
                        );
                    }
                    None => {
                        let page_bg = if p.dark { p.surface } else { Color32::WHITE };
                        let title_bar = if p.dark {
                            p.faint
                        } else {
                            Color32::from_rgb(0xa6, 0xae, 0xb6)
                        };
                        let line_bar = if p.dark {
                            p.border_strong
                        } else {
                            Color32::from_rgb(0xdf, 0xe3, 0xe7)
                        };
                        painter.rect_filled(page, 0, page_bg);
                        let mut y = page.top() + 18.0;
                        let title_w = (page.width() - 28.0) * 0.55;
                        painter.rect_filled(
                            Rect::from_min_size(
                                pos2(page.center().x - title_w / 2.0, y),
                                Vec2::new(title_w, 6.0),
                            ),
                            2,
                            title_bar,
                        );
                        y += 6.0 + 6.0 + 8.0;
                        for _ in 0..4 {
                            painter.rect_filled(
                                Rect::from_min_size(
                                    pos2(page.left() + 14.0, y),
                                    Vec2::new(page.width() - 28.0, 4.0),
                                ),
                                2,
                                line_bar,
                            );
                            y += 12.0;
                        }
                    }
                }
            },
        );
        if r.dragged() {
            painter.rect_filled(
                rect,
                CornerRadius::same(theme::R_LG),
                p.app.gamma_multiply(0.5),
            );
            lift = rect;
        }
        let chip = (!st.query.trim().is_empty())
            .then(|| folder_name(c.groups, &rec.path))
            .flatten();
        let sub = match chip {
            Some(f) => format!("{f} · {}", ago(c.now_ms, rec.last_opened)),
            None => ago(c.now_ms, rec.last_opened),
        };
        card_info(&painter, p, lift, name_of(&rec.path), &sub);
        let (clicked, anchor) = more_button(
            ui,
            p,
            Rect::from_min_size(
                pos2(lift.right() - 8.0 - 26.0, lift.top() + 8.0),
                Vec2::splat(26.0),
            ),
            &format!("Actions for {}", name_of(&rec.path)),
            r.hovered(),
            false,
        );
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name_of(&rec.path)));
        if clicked || r.secondary_clicked() {
            st.project_menu = Some((rec.path.clone(), anchor));
        } else if r.clicked() {
            *action = Some(Action::Open(rec.path.clone()));
        }
    }
}

/// `.open`: a card with a 210 px picture area, lifting 1 px with a shadow on hover. Returns where it was drawn.
fn card_frame(
    ui: &Ui,
    painter: &egui::Painter,
    p: &Palette,
    rect: Rect,
    hovered: bool,
    drop: bool,
    picture: impl FnOnce(&egui::Painter, Rect),
) -> Rect {
    let t = ui.ctx().animate_bool_with_time(
        ui.scope_id()
            .with(("lift", rect.min.x as i32, rect.min.y as i32)),
        hovered,
        0.15,
    );
    let rect = rect.translate(Vec2::new(0.0, -t));
    if t > 0.0 {
        let mut shadow = p.shadow;
        shadow.color = shadow.color.gamma_multiply(t);
        painter.add(shadow.as_shape(rect, CornerRadius::same(theme::R_LG)));
    }
    painter.rect_filled(rect, CornerRadius::same(theme::R_LG), p.surface);
    let thumb = Rect::from_min_size(
        rect.min + Vec2::splat(1.0),
        Vec2::new(rect.width() - 2.0, 210.0),
    );
    let clip = painter.with_clip_rect(thumb);
    picture(&clip, thumb);
    painter.hline(rect.x_range(), thumb.bottom(), Stroke::new(1.0, p.border));
    painter.rect_stroke(
        rect,
        CornerRadius::same(theme::R_LG),
        Stroke::new(1.0, if drop { p.accent } else { p.border }),
        StrokeKind::Inside,
    );
    // The picture's square corners would show past the card's round ones; cover them with the ground.
    rect
}

/// One line of text cut off with an ellipsis at `width`.
fn one_line(
    painter: &egui::Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.into(), font, color);
    job.wrap.max_width = width.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    painter.layout_job(job)
}

fn card_info(painter: &egui::Painter, p: &Palette, rect: Rect, title: &str, sub: &str) {
    let top = rect.top() + 1.0 + 210.0 + 1.0 + 10.0;
    // A long name stops inside the card instead of running over the next one.
    let width = rect.width() - 24.0;
    let title = one_line(
        painter,
        title,
        FontId::new(13.0, theme::font::sans_600()),
        p.ink,
        width,
    );
    painter.galley(pos2(rect.left() + 12.0, top), title, p.ink);
    let sub = one_line(painter, sub, FontId::proportional(12.0), p.muted, width);
    painter.galley(pos2(rect.left() + 12.0, top + 19.0), sub, p.muted);
}

fn commit(st: &mut State, action: &mut Option<Action>) {
    // Any other change to the folders ends the Undo notice.
    st.undo = None;
    *action = Some(Action::Groups);
}

fn start_edit(st: &mut State, c: &Ctx, id: Option<String>, project: Option<String>) {
    st.undo = None;
    let name = id
        .as_ref()
        .and_then(|id| c.groups.groups.iter().find(|g| &g.id == id))
        .map(|g| g.name.clone())
        .unwrap_or_default();
    st.editing = Some(Editing {
        id,
        name,
        error: None,
        project,
        focused: false,
    });
}

fn remove_group(st: &mut State, c: &mut Ctx, id: &str, action: &mut Option<Action>) {
    let before = c.groups.clone();
    let name = before
        .groups
        .iter()
        .find(|g| g.id == id)
        .map(|g| g.name.clone())
        .unwrap_or_default();
    c.groups.delete(id);
    commit(st, action);
    if st.view == View::Group(id.into()) {
        st.view = View::Root;
    }
    st.undo = Some(Undo {
        id: id.into(),
        name,
        before,
        at: Instant::now(),
    });
}

fn overlays(
    ctx: &egui::Context,
    p: &Palette,
    st: &mut State,
    c: &mut Ctx,
    action: &mut Option<Action>,
) {
    if std::mem::take(&mut st.undo_requested)
        && let Some(u) = st.undo.take()
    {
        c.groups.restore(&u.before, &u.id);
        *action = Some(Action::Groups);
    }
    if let Some((path, anchor)) = st.project_menu.clone() {
        let items = [
            MenuItem::new(icons::EXTERNAL_LINK, "Open"),
            MenuItem::new(icons::FOLDER_SEARCH, "Show in file manager"),
            MenuItem::new(icons::LAYOUT_TEMPLATE, "Save as template…"),
            MenuItem::new(icons::FOLDER, "Move to folder…"),
            MenuItem::Separator,
            MenuItem::new(icons::X, "Remove from list").hint("keeps files"),
            MenuItem::new(icons::TRASH, "Delete project…").danger(),
        ];
        match widgets::drop_menu(
            ctx,
            p,
            egui::Id::unique("project-menu"),
            anchor,
            true,
            &items,
        ) {
            MenuResult::Open => {}
            MenuResult::Closed => st.project_menu = None,
            MenuResult::Clicked(i) => {
                st.project_menu = None;
                match i {
                    0 => *action = Some(Action::Open(path)),
                    1 => *action = Some(Action::Reveal(path)),
                    2 => *action = Some(Action::SaveTemplate(path)),
                    3 => st.moving = Some(path),
                    5 => {
                        // Removing a project from the list also takes it out of its folder.
                        c.groups.forget(&path);
                        st.undo = None;
                        *action = Some(Action::Forget(path));
                    }
                    _ => {
                        st.deleting = Some(Deleting {
                            path,
                            busy: false,
                            error: None,
                        })
                    }
                }
            }
        }
    }
    if let Some((id, anchor)) = st.folder_menu.clone() {
        let items = [
            MenuItem::new(icons::FOLDER_OPEN, "Open"),
            MenuItem::new(icons::SQUARE_PEN, "Rename…"),
            MenuItem::new(icons::TRASH, "Delete folder")
                .hint("keeps projects")
                .danger(),
        ];
        match widgets::drop_menu(
            ctx,
            p,
            egui::Id::unique("folder-menu"),
            anchor,
            true,
            &items,
        ) {
            MenuResult::Open => {}
            MenuResult::Closed => st.folder_menu = None,
            MenuResult::Clicked(i) => {
                st.folder_menu = None;
                match i {
                    0 => {
                        st.recent_only = false;
                        st.view = View::Group(id);
                    }
                    1 => start_edit(st, c, Some(id), None),
                    _ => remove_group(st, c, &id, action),
                }
            }
        }
    }
    folder_name_dialog(ctx, p, st, c, action);
    move_dialog(ctx, p, st, c, action);
    delete_dialog(ctx, p, st, action);
    if let Some((note, _)) = st.toast.clone() {
        let screen = ctx.content_rect();
        let mut dismiss = false;
        egui::Area::new(egui::Id::unique("library-toast"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(screen.center().x, screen.bottom() - 20.0))
            .show(ctx, |ui| {
                ui.set_max_width((screen.width() * 0.9).min(560.0));
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(theme::R_LG)
                    .shadow(p.shadow)
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 8,
                        top: 8,
                        bottom: 8,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            icons::show(ui, icons::CIRCLE_CHECK, 16.0, p.ok);
                            ui.add(
                                egui::Label::new(egui::RichText::new(&note).color(p.ink)).wrap(),
                            );
                            if Button::icon(icons::X, "Dismiss")
                                .size(Size::Sm)
                                .icon_size(14.0)
                                .show_icon(ui, p)
                                .clicked()
                            {
                                dismiss = true;
                            }
                        });
                    });
            });
        if dismiss {
            st.toast = None;
        }
    }
}

fn folder_name_dialog(
    ctx: &egui::Context,
    p: &Palette,
    st: &mut State,
    c: &mut Ctx,
    action: &mut Option<Action>,
) {
    let Some(ed) = st.editing.as_mut() else {
        return;
    };
    let title = if ed.id.is_some() {
        "Rename folder"
    } else {
        "New folder"
    };
    let mut enter = false;
    let mut confirmed = false;
    let mut cancel = false;
    let can = !ed.name.trim().is_empty();
    let confirm = if ed.id.is_some() { "Rename" } else { "Create" };
    let r = widgets::modal(
        ctx,
        p,
        title,
        380.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let input = widgets::text_input(ui, p, &mut ed.name, "Folder name", false);
            ui.ctx()
                .accesskit_node_builder(input.id, |n| n.set_label("Folder name"));
            if !ed.focused {
                input.request_focus();
                ed.focused = true;
            }
            if input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                enter = true;
            }
            if let Some(e) = &ed.error {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(e).size(11.5).color(p.error));
                });
            }
        },
        Some(&mut |ui: &mut Ui| {
            confirmed |= Button::new(confirm)
                .kind(Kind::Primary)
                .enabled(can)
                .show(ui, p)
                .clicked();
            cancel |= Button::new("Cancel").show(ui, p).clicked();
        }),
    );
    if r.closed || cancel {
        st.editing = None;
        return;
    }
    if enter || confirmed {
        let ed = st.editing.as_mut().unwrap();
        if let Some(problem) = c.groups.name_problem(&ed.name, ed.id.as_deref()) {
            ed.error = Some(problem);
            return;
        }
        let ed = st.editing.take().unwrap();
        match ed.id {
            Some(id) => {
                c.groups.rename(&id, &ed.name);
            }
            None => {
                if let Some(id) = c.groups.create(&ed.name, None)
                    && let Some(project) = ed.project
                {
                    c.groups.move_project(&project, Some(&id));
                }
            }
        }
        commit(st, action);
    }
}

fn move_dialog(
    ctx: &egui::Context,
    p: &Palette,
    st: &mut State,
    c: &mut Ctx,
    action: &mut Option<Action>,
) {
    let Some(path) = st.moving.clone() else {
        return;
    };
    let mut groups: Vec<(Option<String>, String)> = vec![(None, "No folder".into())];
    let mut sorted = c.groups.groups.clone();
    sorted.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    groups.extend(sorted.into_iter().map(|g| (Some(g.id), g.name)));
    let current = c.groups.members.get(&path).cloned();
    let mut chosen: Option<Option<String>> = None;
    let mut new_folder = false;
    let r = widgets::modal(
        ctx,
        p,
        &format!("Move {}", name_of(&path)),
        380.0,
        |ui| {
            let max_h = (ctx.content_rect().height() * 0.55).clamp(140.0, 360.0);
            egui::ScrollArea::vertical()
                .id_salt("move-folders")
                .max_height(max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (id, name) in &groups {
                        let here = *id == current;
                        let icon = if id.is_some() {
                            icons::FOLDER
                        } else {
                            icons::FILE_TEXT
                        };
                        let r = list_button(ui, p, icon, name, here);
                        r.widget_info(|| {
                            egui::WidgetInfo::selected(egui::Role::RadioButton, true, here, name)
                        });
                        if r.clicked() {
                            chosen = Some(id.clone());
                        }
                    }
                    if list_button(ui, p, icons::FOLDER_PLUS, "New folder…", false).clicked() {
                        new_folder = true;
                    }
                });
        },
        None,
    );
    if let Some(id) = chosen {
        c.groups.move_project(&path, id.as_deref());
        commit(st, action);
        st.moving = None;
    } else if new_folder {
        st.moving = None;
        start_edit(st, c, None, Some(path));
    } else if r.closed {
        st.moving = None;
    }
}

/// `.moves button`: a 34 px row with an icon, `--accent-soft` when it's the current choice.
fn list_button(ui: &mut Ui, p: &Palette, icon: Icon, label: &str, on: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let fill = if on {
        p.accent_soft
    } else if r.hovered() {
        p.surface_2
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::R), fill);
    icons::paint(
        ui.painter(),
        Rect::from_min_size(
            pos2(rect.left() + 10.0, rect.center().y - 7.5),
            Vec2::splat(15.0),
        ),
        icon,
        p.muted,
    );
    ui.painter().text(
        pos2(rect.left() + 35.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        p.ink,
    );
    widgets::focus_ring(ui, &r, rect, p, theme::R);
    r
}

fn delete_dialog(ctx: &egui::Context, p: &Palette, st: &mut State, action: &mut Option<Action>) {
    let Some(d) = st.deleting.as_ref() else {
        return;
    };
    let busy = d.busy;
    let path = d.path.clone();
    let error = d.error.clone();
    let mut go = false;
    let mut cancel = false;
    let r = widgets::modal(
        ctx,
        p,
        &format!("Delete {}?", name_of(&path)),
        460.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(egui::RichText::new("This moves the folder and everything in it to your system's trash. You can restore it from there.").color(p.muted));
            egui::Frame::new()
                .fill(p.surface_2)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(theme::R)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&path)
                                .font(FontId::monospace(12.0))
                                .color(p.ink),
                        )
                        .wrap(),
                    );
                });
            if let Some(e) = &error {
                ui.label(egui::RichText::new(e).color(p.error));
            }
        },
        Some(&mut |ui: &mut Ui| {
            go |= Button::new("Move to trash")
                .kind(Kind::Primary)
                .ink(p.accent_ink)
                .fill(p.error)
                .enabled(!busy)
                .show(ui, p)
                .clicked();
            cancel |= Button::new("Cancel").enabled(!busy).show(ui, p).clicked();
        }),
    );
    if go {
        st.deleting = Some(Deleting {
            path: path.clone(),
            busy: true,
            error: None,
        });
        *action = Some(Action::Trash(path));
    } else if (r.closed && !busy) || cancel {
        st.deleting = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_times_read_like_intl() {
        let now = 1_000_000_000_000.0;
        let ago_s = |s: f64| ago(now, now - s * 1000.0);
        assert_eq!(ago(now, 0.0), "—");
        assert_eq!(ago_s(10.0), "just now");
        assert_eq!(ago_s(3600.0), "1 hour ago");
        assert_eq!(ago_s(5.0 * 60.0), "5 minutes ago");
        assert_eq!(ago_s(86400.0), "yesterday");
        assert_eq!(ago_s(3.0 * 86400.0), "3 days ago");
        assert_eq!(ago_s(20.0 * 86400.0), "3 weeks ago");
        assert_eq!(ago_s(8.0 * 86400.0), "last week");
        assert_eq!(ago_s(400.0 * 86400.0), "last year");
    }

    #[test]
    fn paths_show_name_folder_and_home() {
        assert_eq!(
            name_of("/home/you/Documents/papers/rl-thesis/"),
            "rl-thesis"
        );
        assert_eq!(
            folder_of("/home/you/Documents/papers/rl-thesis"),
            "/home/you/Documents/papers"
        );
        assert_eq!(home("/home/you/Documents/uni"), "~/Documents/uni");
        assert_eq!(home("/Users/you/Documents"), "~/Documents");
        assert_eq!(home(r"C:\Users\you\Documents"), r"~\Documents");
        assert_eq!(home("/opt/tex"), "/opt/tex");
    }

    #[test]
    fn natural_order_ignores_case_and_reads_numbers() {
        let mut v = vec![
            "NET_assignment_10",
            "cv",
            "CA_Assignement_4",
            "NET_assignment_4",
        ];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            [
                "CA_Assignement_4",
                "cv",
                "NET_assignment_4",
                "NET_assignment_10"
            ]
        );
    }
}

#[cfg(test)]
mod snapshots {
    use super::*;
    use crate::testing;
    use egui_kittest::kittest::Queryable;

    fn render(p: Palette, view: &str, name: &str) -> egui_kittest::SnapshotResult {
        let now = 1_800_000_000_000.0;
        let recents = testing::recents(now);
        let mut groups = Groups::default();
        let mut settings = Settings {
            library_view: view.into(),
            ..Settings::default()
        };
        let mut st = State::default();
        let mut h = testing::harness(p, move |ui| {
            let mut c = Ctx {
                recents: &recents,
                groups: &mut groups,
                settings: &mut settings,
                error: None,
                no_main_in: None,
                thumbnail: std::sync::Arc::new(|_| None),
                now_ms: now,
            };
            show(ui, &p, &mut st, &mut c);
        });
        h.run();
        h.try_snapshot(name)
    }

    #[test]
    fn a_long_project_name_is_cut_to_the_width_it_has() {
        let width = std::sync::Arc::new(std::sync::Mutex::new(0.0));
        let w = width.clone();
        let mut h = testing::harness(theme::LIGHT, move |ui| {
            let long = "Master_Thesis_Final_Version_Corrected_2024_v3";
            let g = one_line(
                ui.painter(),
                long,
                FontId::proportional(13.0),
                Color32::BLACK,
                90.0,
            );
            *w.lock().unwrap() = g.size().x;
        });
        h.run();
        let got = *width.lock().unwrap();
        assert!(got > 0.0 && got <= 90.0, "{got}");
    }

    #[test]
    fn long_folder_name_stops_before_the_tools() {
        let name = "Computer Architecture and Operating Syst";
        let mut groups = Groups::default();
        let id = groups.create(name, None).unwrap();
        let mut st = State {
            view: View::Group(id),
            ..State::default()
        };
        let recents = testing::recents(0.0);
        let mut settings = Settings::default();
        let mut h = testing::harness(theme::LIGHT, move |ui| {
            let mut c = Ctx {
                recents: &recents,
                groups: &mut groups,
                settings: &mut settings,
                error: None,
                no_main_in: None,
                thumbnail: std::sync::Arc::new(|_| None),
                now_ms: 0.0,
            };
            show(ui, &theme::LIGHT, &mut st, &mut c);
        });
        h.run();
        let title = h
            .get_by_role_and_label(egui::accesskit::Role::Label, name)
            .rect();
        let rename = h.get_by_label("Rename").rect();
        assert!(
            title.width() > 0.0 && title.right() <= rename.left(),
            "{title:?} runs under {rename:?}"
        );
        h.try_snapshot("library-long-folder-light").unwrap();
    }

    #[test]
    fn every_thumbnail_arrives_after_decoding() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(60, 76, image::Rgba([200, 30, 30, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let recents = testing::recents(0.0);
        let n = recents.0.len();
        let st = std::sync::Arc::new(std::sync::Mutex::new(State::default()));
        let shared = st.clone();
        let mut groups = Groups::default();
        let mut settings = Settings {
            library_view: "grid".into(),
            ..Settings::default()
        };
        let caller = std::thread::current().id();
        let read_threads = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reads = read_threads.clone();
        let mut h = testing::harness(theme::LIGHT, move |ui| {
            let reads = reads.clone();
            let png = png.clone();
            let mut c = Ctx {
                recents: &recents,
                groups: &mut groups,
                settings: &mut settings,
                error: None,
                no_main_in: None,
                thumbnail: std::sync::Arc::new(move |_| {
                    reads.lock().unwrap().push(std::thread::current().id());
                    Some(png.clone())
                }),
                now_ms: 0.0,
            };
            show(ui, &theme::LIGHT, &mut shared.lock().unwrap(), &mut c);
        });
        let start = std::time::Instant::now();
        while st.lock().unwrap().thumbs.values().flatten().count() < n
            && start.elapsed() < std::time::Duration::from_secs(5)
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
            h.step();
        }
        assert_eq!(st.lock().unwrap().thumbs.values().flatten().count(), n);
        let threads = read_threads.lock().unwrap();
        assert!(!threads.is_empty());
        assert!(threads.iter().all(|thread| *thread != caller));
    }

    #[test]
    fn library_list_and_grid() {
        let mut results = egui_kittest::SnapshotResults::new();
        for (p, scheme) in [(theme::LIGHT, "light"), (theme::DARK, "dark")] {
            results.add(render(p, "list", &format!("library-list-{scheme}")));
            results.add(render(p, "grid", &format!("library-grid-{scheme}")));
        }
    }

    #[test]
    fn home_never_panics_on_non_ascii_windows_or_unix_paths() {
        // Direct regression for WS-01 plus systematic multi-byte UTF-8 boundary sweep.
        assert_eq!(home(r"D:\Work\Übung"), r"D:\Work\Übung");
        assert_eq!(home(r"C:\Users\Üser\paper"), r"~\paper");
        let prefixes = [
            "C:\\", "D:\\", "Z:\\", "c:\\", "/home/", "/Users/", "Ü:\\", "",
        ];
        let segments = ["Work", "Users", "Übung", "café", "日本語", "🦀", "a", ""];
        for pfx in prefixes {
            for &s1 in &segments {
                for &s2 in &segments {
                    let candidate = format!("{pfx}{s1}\\{s2}");
                    let _ = home(&candidate);
                    let _ = folder_of(&candidate);
                    let unix = format!("{pfx}{s1}/{s2}");
                    let _ = home(&unix);
                    let _ = folder_of(&unix);
                }
            }
        }
    }
}
