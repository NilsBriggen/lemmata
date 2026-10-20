//! Insert figure (FigureDialog.svelte), insert table (TableDialog.svelte), and pasted screenshots.

use super::{Tone, Workspace};
use crate::backend::{Backend, Msg};
use crate::icons;
use crate::theme::Palette;
use crate::widgets::{self, Button, Kind};
use crate::writing::table;
use eframe::egui::{self, CornerRadius, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

const IMAGE_EXTS: [&str; 8] = ["png", "jpg", "jpeg", "pdf", "svg", "eps", "gif", "webp"];

pub struct FigureDialog {
    path: String,
    caption: String,
    label: String,
    width: String,
    placement: String,
    busy: bool,
    error: Option<String>,
}

pub struct TableDialog {
    rows: usize,
    cols: usize,
    rows_text: String,
    cols_text: String,
    hover: Option<(usize, usize)>,
    header: bool,
    booktabs: bool,
    caption: String,
    label: String,
}

/// The project's images, for the Figure dialog's list.
fn images(entries: &[lemmata_project::Entry], out: &mut Vec<String>) {
    for e in entries {
        if e.is_dir {
            images(&e.children, out);
        } else if e
            .path
            .rsplit_once('.')
            .is_some_and(|(_, x)| IMAGE_EXTS.contains(&x.to_lowercase().as_str()))
        {
            out.push(e.path.clone());
        }
    }
}

impl Workspace {
    pub fn open_figure(&mut self) {
        let mut list = Vec::new();
        images(&self.info.tree, &mut list);
        self.figure = Some(FigureDialog {
            path: list.first().cloned().unwrap_or_default(),
            caption: String::new(),
            label: String::new(),
            width: "80".into(),
            placement: "htbp".into(),
            busy: false,
            error: None,
        });
    }

    pub fn open_table(&mut self) {
        self.table = Some(TableDialog {
            rows: 3,
            cols: 3,
            rows_text: "3".into(),
            cols_text: "3".into(),
            hover: None,
            header: true,
            booktabs: true,
            caption: String::new(),
            label: String::new(),
        });
    }

    /// A screenshot pasted into a .tex file: saved in the project, then inserted as a figure.
    pub(super) fn paste_image(&mut self, backend: &Backend, path: &str, img: &egui::ColorImage) {
        let mut png = Vec::new();
        let rgba: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|c| c.to_srgba_unmultiplied())
            .collect();
        let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba)
        else {
            return;
        };
        if let Err(e) = buf.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png) {
            self.notice(e.to_string(), Tone::Error);
            return;
        }
        let file = path.to_owned();
        Workspace::job(
            backend,
            move |shell| shell.save_pasted_image(&png),
            move |ws, b, r| match r {
                Ok(rel) => {
                    b.spawn(|shell| Msg::Info(shell.project_info_cmd()));
                    let indent = ws.indent;
                    if let Some(ed) = ws.editors.get_mut(&file) {
                        ed.insert_template(&table::figure_snippet(&rel), indent);
                    }
                    ws.needs_package(b, "graphicx".into());
                }
                Err(e) => ws.notice(e, Tone::Error),
            },
        );
    }

    /// Format document: latexindent or tex-fmt over the whole file, as one step Undo takes back.
    pub fn format_document(&mut self, backend: &Backend) {
        let Some(path) = self.active.clone().filter(|a| self.editors.contains_key(a)) else {
            return;
        };
        let text = self.editors[&path].doc.text();
        let unit = self.indent.unit();
        Workspace::job(
            backend,
            move |s| s.format_tex(text.clone(), unit).map(|f| (text, f)),
            move |ws, _, r| match r {
                Ok((before, formatted)) => {
                    let Some(ed) = ws.editors.get_mut(&path) else {
                        return;
                    };
                    // Typing while it ran: the formatted text is out of date.
                    if ed.doc.text() != before {
                        return;
                    }
                    if formatted != before {
                        ed.doc.replace_all(&formatted);
                        crate::editor::view::after_command(ed);
                    }
                    ws.notice(
                        "Formatted the document. Undo brings the old layout back.",
                        Tone::Ok,
                    );
                }
                Err(e) => ws.notice(e, Tone::Error),
            },
        );
    }

    /// Images dropped onto a .tex file: each copied into the project and inserted as a figure
    /// where it was dropped.
    pub(super) fn drop_figures(
        &mut self,
        backend: &Backend,
        path: &str,
        files: Vec<String>,
        at: usize,
    ) {
        let file = path.to_owned();
        Workspace::job(
            backend,
            move |s| {
                files
                    .into_iter()
                    .map(|f| s.import_figure(f))
                    .collect::<Vec<_>>()
            },
            move |ws, b, results| {
                let indent = ws.indent;
                for r in results {
                    match r {
                        Ok(rel) => {
                            if let Some(ed) = ws.editors.get_mut(&file) {
                                let at = at.min(ed.doc.len());
                                ed.doc.selection = crate::editor::buffer::Selection::cursor(at);
                                ed.insert_template(&table::figure_snippet(&rel), indent);
                                ed.refocus = true;
                            }
                            ws.needs_package(b, "graphicx".into());
                        }
                        Err(e) => ws.notice(e, Tone::Error),
                    }
                }
                b.spawn(|shell| Msg::Info(shell.project_info_cmd()));
            },
        );
    }

    /// "Browse…": an image from the computer, copied into the project.
    fn browse_figure(&mut self, backend: &Backend) {
        if let Some(f) = &mut self.figure {
            f.busy = true;
            f.error = None;
        }
        backend.spawn_async(|shell| async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Choose an image")
                .add_filter("Images", &IMAGE_EXTS)
                .pick_file()
                .await
                .map(|h| h.path().to_string_lossy().into_owned());
            let r = picked.map(|p| shell.import_figure(p));
            Msg::Apply(Box::new(move |ws: &mut Workspace, _: &Backend| {
                let Some(f) = &mut ws.figure else { return };
                f.busy = false;
                match r {
                    Some(Ok(path)) => f.path = path,
                    Some(Err(e)) => f.error = Some(e),
                    None => {}
                }
            }) as crate::backend::Job)
        });
    }

    pub(super) fn insert_dialogs(&mut self, ctx: &egui::Context, p: &Palette, backend: &Backend) {
        if let Some(f) = &mut self.figure {
            let mut list = Vec::new();
            images(&self.info.tree, &mut list);
            let (mut insert, mut cancel, mut browse) = (false, false, false);
            let can = !f.path.is_empty();
            let r = widgets::modal_with(
                ctx,
                p,
                "Insert figure",
                Some("Images stay in your project's folder."),
                520.0,
                |ui| figure_body(ui, p, f, &list, &mut browse),
                Some(&mut |ui: &mut Ui| {
                    if Button::new("Insert")
                        .kind(Kind::Primary)
                        .enabled(can)
                        .show(ui, p)
                        .clicked()
                    {
                        insert = true;
                    }
                    if Button::new("Cancel").show(ui, p).clicked() {
                        cancel = true;
                    }
                }),
            );
            if insert && !f.path.is_empty() {
                let text = table::figure_block(&table::FigureOptions {
                    path: &f.path,
                    caption: &f.caption,
                    label: &f.label,
                    width: f.width.parse().unwrap_or(80),
                    placement: &f.placement,
                });
                self.figure = None;
                let indent = self.indent;
                if let Some(ed) = self.active_editor() {
                    ed.insert_literal_block(&format!("{text}\n"), indent);
                }
                self.needs_package(backend, "graphicx".into());
            } else if r.closed || cancel {
                self.figure = None;
            } else if browse {
                self.browse_figure(backend);
            }
        }
        if let Some(t) = &mut self.table {
            let (mut insert, mut cancel) = (false, false);
            let r = widgets::modal_with(
                ctx,
                p,
                "Insert table",
                None,
                460.0,
                |ui| table_body(ui, p, t),
                Some(&mut |ui: &mut Ui| {
                    if Button::new("Insert")
                        .kind(Kind::Primary)
                        .show(ui, p)
                        .clicked()
                    {
                        insert = true;
                    }
                    if Button::new("Cancel").show(ui, p).clicked() {
                        cancel = true;
                    }
                }),
            );
            if insert {
                let o = table::TableOptions {
                    rows: t.rows,
                    cols: t.cols,
                    header: t.header,
                    booktabs: t.booktabs,
                    caption: &t.caption,
                    label: &t.label,
                };
                let text = table::table_block(&o, true);
                let booktabs = t.booktabs;
                self.table = None;
                let indent = self.indent;
                if let Some(ed) = self.active_editor() {
                    ed.insert_template(&format!("{text}\n"), indent);
                    ed.refocus = true;
                }
                if booktabs {
                    self.needs_package(backend, "booktabs".into());
                }
            } else if r.closed || cancel {
                self.table = None;
            }
        }
    }
}

fn figure_body(ui: &mut Ui, p: &Palette, f: &mut FigureDialog, list: &[String], browse: &mut bool) {
    widgets::field(ui, p, "Image", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let bw = 100.0;
            ui.allocate_ui(vec2(ui.available_width() - bw - 8.0, 32.0), |ui| {
                if list.is_empty() && f.path.is_empty() {
                    ui.add_sized(
                        vec2(ui.available_width(), 32.0),
                        egui::Label::new(
                            egui::RichText::new("No images in this project yet.").color(p.muted),
                        ),
                    );
                } else {
                    let mut opts: Vec<(&str, &str)> = Vec::new();
                    if !f.path.is_empty() && !list.contains(&f.path) {
                        opts.push((&f.path, &f.path));
                    }
                    opts.extend(list.iter().map(|x| (x.as_str(), x.as_str())));
                    let mut v = f.path.clone();
                    widgets::select(ui, p, "Image", &mut v, &opts);
                    f.path = v;
                }
            });
            if Button::new("Browse…")
                .with_icon(icons::FOLDER_OPEN)
                .enabled(!f.busy)
                .show(ui, p)
                .clicked()
            {
                *browse = true;
            }
        });
        if let Some(e) = &f.error {
            ui.label(egui::RichText::new(e).size(12.0).color(p.error));
        }
    });
    widgets::field(ui, p, "Caption", |ui| {
        widgets::text_input(ui, p, &mut f.caption, "Optional", false)
    });
    ui.columns(2, |c| {
        widgets::field(&mut c[0], p, "Label", |ui| {
            widgets::text_input(ui, p, &mut f.label, "made from the caption", true)
        });
        widgets::field(&mut c[1], p, "Width", |ui| {
            widgets::select(
                ui,
                p,
                "Width",
                &mut f.width,
                &[
                    ("30", "30% of the text width"),
                    ("50", "50% of the text width"),
                    ("80", "80% of the text width"),
                    ("100", "100% of the text width"),
                ],
            )
        });
    });
    widgets::field(ui, p, "Placement", |ui| {
        const PLACES: [(&str, &str); 4] = [
            ("htbp", "Automatic"),
            ("H", "Right here"),
            ("t", "Top"),
            ("", "Default"),
        ];
        let options: Vec<(Option<icons::Icon>, &str)> =
            PLACES.iter().map(|(_, n)| (None, *n)).collect();
        let sel = PLACES
            .iter()
            .position(|(v, _)| *v == f.placement)
            .unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, p, &options, sel, false, false) {
            f.placement = PLACES[i].0.into();
        }
    });
}

fn table_body(ui: &mut Ui, p: &Palette, t: &mut TableDialog) {
    ui.spacing_mut().item_spacing.y = 12.0;
    let (sr, sc) = t.hover.unwrap_or((t.rows, t.cols));
    widgets::field(ui, p, &format!("Size: {sc} × {sr}"), |ui| {
        // An 8 × 8 grid of 22 px cells, 3 px apart.
        const MAX: usize = 8;
        let (area, _) = ui.allocate_exact_size(
            vec2(MAX as f32 * 25.0 - 3.0, MAX as f32 * 25.0 - 3.0),
            Sense::hover(),
        );
        let mut hovered = None;
        for r in 1..=MAX {
            for c in 1..=MAX {
                let rect = Rect::from_min_size(
                    pos2(
                        area.left() + (c - 1) as f32 * 25.0,
                        area.top() + (r - 1) as f32 * 25.0,
                    ),
                    vec2(22.0, 22.0),
                );
                let resp = ui.interact(rect, ui.scope_id().with(("cell", r, c)), Sense::click());
                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        format!("{c} columns by {r} rows"),
                    )
                });
                if resp.hovered() {
                    hovered = Some((r, c));
                }
                if resp.clicked() {
                    t.rows = r;
                    t.cols = c;
                    t.rows_text = r.to_string();
                    t.cols_text = c.to_string();
                }
                let on = r <= sr && c <= sc;
                ui.painter().rect(
                    rect,
                    CornerRadius::same(4),
                    if on { p.accent_soft } else { p.surface },
                    Stroke::new(1.0, if on { p.accent } else { p.border_strong }),
                    StrokeKind::Inside,
                );
            }
        }
        t.hover = hovered;
    });
    ui.columns(2, |c| {
        widgets::field(&mut c[0], p, "Columns", |ui| {
            widgets::text_input(ui, p, &mut t.cols_text, "", false);
        });
        widgets::field(&mut c[1], p, "Rows", |ui| {
            widgets::text_input(ui, p, &mut t.rows_text, "", false);
        });
    });
    if let Ok(n) = t.cols_text.trim().parse::<usize>() {
        t.cols = n.clamp(1, 20);
    }
    if let Ok(n) = t.rows_text.trim().parse::<usize>() {
        t.rows = n.clamp(1, 100);
    }
    widgets::checkbox(ui, p, &mut t.header, "First row is a header");
    widgets::checkbox(ui, p, &mut t.booktabs, "Clean rules (booktabs)");
    widgets::field(ui, p, "Caption", |ui| {
        widgets::text_input(
            ui,
            p,
            &mut t.caption,
            "Leave empty to type it in the document",
            false,
        )
    });
    widgets::field(ui, p, "Label", |ui| {
        widgets::text_input(ui, p, &mut t.label, "made from the caption", true)
    });
}
