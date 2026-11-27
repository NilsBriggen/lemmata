//! Pure-Rust exporter that produces a shareable PDF combining the rendered slides, crisp PDF
//! vector ink strokes drawn during the presentation, and (optionally) speaker notes below each
//! slide — without ever modifying the original `.tex` source.

use super::data::{StrokePath, WhiteboardSlide, region_size, regions_swapped};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use image::{ColorType, ImageEncoder};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::Arc;

/// Exports an annotated PDF from the original PDF bytes, drawn ink strokes, and optional speaker
/// notes.
///
/// - When `include_notes` is `false`, each output page matches the slide's exact size with native
///   PDF vector ink strokes overlaid on top.
/// - When `include_notes` is `true`, each output page is a clean handout layout: the annotated
///   slide on top plus either Beamer's compiled second-screen notes half or the speaker's custom
///   notes text neatly formatted below.
pub fn export_annotated_pdf(
    pdf_bytes: &[u8],
    annotations: &BTreeMap<usize, Vec<StrokePath>>,
    notes: &BTreeMap<usize, String>,
    whiteboards: &[WhiteboardSlide],
    include_notes: bool,
    swap_notes_half: bool,
) -> Result<Vec<u8>, String> {
    let pdf = Pdf::new(Arc::new(pdf_bytes.to_vec()))
        .map_err(|_| "Could not read the source PDF for export.".to_owned())?;
    let pages = pdf.pages();
    if pages.is_empty() {
        return Err("The PDF has no pages to export.".to_owned());
    }

    let pdf_total = pages.len();
    let total = pdf_total + whiteboards.len();
    let cache = RenderCache::new();
    let mut writer = PdfWriter::new();
    let font_id = writer.add_object(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    let font_bold_id = writer.add_object(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    let gs_opaque_id = writer.add_object(b"<< /Type /ExtGState /CA 1.0 /ca 1.0 >>".to_vec());
    let gs_highlight_id = writer.add_object(b"<< /Type /ExtGState /CA 0.35 /ca 0.35 >>".to_vec());

    let mut page_ids = Vec::with_capacity(total);
    // Reserve the /Pages tree object id up front so every page can reference `/Parent`.
    let pages_root_id = writer.reserve_id();

    let (default_sw, default_sh) = {
        let (pw, ph) = pages[0].render_dimensions();
        let (sr, _) = regions_swapped(pw, ph, swap_notes_half);
        let (sw, sh, _) = region_size(pw, ph, sr);
        if sw.is_finite() && sh.is_finite() && sw > 1.0 && sh > 1.0 {
            (sw, sh)
        } else {
            (455.0, 256.0)
        }
    };

    let mut display_num = 0usize;
    let push_whiteboards = |after_pdf_page: usize,
                            or_past_end: bool,
                            display_num: &mut usize,
                            writer: &mut PdfWriter,
                            page_ids: &mut Vec<usize>| {
        for wb in whiteboards {
            let matches = wb.after_pdf_page == after_pdf_page
                || (or_past_end && wb.after_pdf_page > pdf_total);
            if !matches {
                continue;
            }
            *display_num += 1;
            let pid = emit_whiteboard_page(
                writer,
                pages_root_id,
                font_id,
                font_bold_id,
                gs_opaque_id,
                gs_highlight_id,
                wb,
                default_sw,
                default_sh,
                *display_num,
                total,
                include_notes,
            );
            page_ids.push(pid);
        }
    };

    push_whiteboards(0, false, &mut display_num, &mut writer, &mut page_ids);

    for (idx, page) in pages.iter().enumerate() {
        let slide_num = idx + 1;
        display_num += 1;
        let (pw, ph) = page.render_dimensions();
        if !pw.is_finite() || !ph.is_finite() || pw <= 0.0 || ph <= 0.0 {
            return Err(format!("Slide {slide_num} has invalid PDF dimensions."));
        }
        let (slide_region, notes_region) = regions_swapped(pw, ph, swap_notes_half);
        let (sw, sh, sx) = region_size(pw, ph, slide_region);

        // Render at ~2.2x PDF points (crisp ~160-220 DPI), clamped to keep export fast & compact.
        let max_dim = pw.max(ph).max(1.0);
        let scale = (2.2f32).min(2400.0 / max_dim).max(1.0);
        let pix_settings = PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        };
        let interp = InterpreterSettings::default();
        let pixmap = render(
            page,
            &cache,
            &interp,
            &RenderSettings::default(),
            &pix_settings,
        );
        let full_w = pixmap.width() as usize;
        let full_h = pixmap.height() as usize;
        let rgba = pixmap.data_as_u8_slice();

        // Crop slide region and JPEG-encode it.
        let (slide_jpeg, slide_px_w, slide_px_h) =
            crop_to_jpeg(rgba, full_w, full_h, sx / pw, sw / pw)?;
        let slide_img_id = writer.add_jpeg_xobject(&slide_jpeg, slide_px_w, slide_px_h);

        let custom_note = notes
            .get(&slide_num)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let beamer_notes_img = if include_notes {
            if let Some(nr) = notes_region {
                let (nw, _nh, nx) = region_size(pw, ph, nr);
                let (nj, nw_px, nh_px) = crop_to_jpeg(rgba, full_w, full_h, nx / pw, nw / pw)?;
                Some((writer.add_jpeg_xobject(&nj, nw_px, nh_px), nw, ph))
            } else {
                None
            }
        } else {
            None
        };

        let strokes = annotations
            .get(&slide_num)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        // Output page geometry in PDF points (origin at bottom-left).
        let mut content = String::with_capacity(2048);
        let (out_w, out_h, slide_rect_pdf) = if !include_notes {
            // Exact slide size, full bleed.
            (sw, sh, (0.0f32, 0.0f32, sw, sh))
        } else {
            // Handout page: slide framed on top, speaker notes cleanly below.
            let margin = (sw * 0.055).clamp(18.0, 36.0);
            let header_h = 20.0f32;
            let gap = 14.0f32;
            let inner_w = sw;
            let out_w = inner_w + margin * 2.0;

            let wrapped_notes =
                custom_note.map(|text| wrap_pdf_lines(text, (inner_w - 24.0).max(24.0), 10.5));
            let notes_h = if let Some((_, nw, nh)) = beamer_notes_img {
                let aspect = if nw > 0.0 { nh / nw } else { 0.75 };
                (inner_w * aspect).min(sh * 0.95)
            } else if let Some(ref wrapped) = wrapped_notes {
                let max_h = (sh * 1.1).max(96.0);
                ((wrapped.len().max(2) as f32) * 14.5 + 34.0).clamp(96.0, max_h)
            } else {
                72.0
            };

            let out_h = margin + notes_h + gap + sh + header_h + margin;
            let slide_x = margin;
            let slide_y = margin + notes_h + gap;

            // Soft paper background.
            let _ = writeln!(
                content,
                "q 0.965 0.969 0.965 rg 0 0 {out_w:.2} {out_h:.2} re f Q"
            );
            // Header: "Slide N / M"
            let header_y = out_h - margin - 12.0;
            let _ = writeln!(
                content,
                "q 0.36 0.41 0.47 rg BT /F2 9.5 Tf {slide_x:.2} {header_y:.2} Td ({}) Tj ET Q",
                pdf_escape_winansi(&format!("Slide {display_num} / {total}"))
            );

            // Notes card below slide.
            let notes_x = margin;
            let notes_y = margin;
            let _ = writeln!(
                content,
                "q 1 1 1 rg 0.85 0.87 0.89 RG 0.6 w {notes_x:.2} {notes_y:.2} {inner_w:.2} {notes_h:.2} re B Q"
            );
            if let Some((n_img_id, nw, nh)) = beamer_notes_img {
                let k = ((inner_w - 8.0) / nw.max(1.0)).min((notes_h - 8.0) / nh.max(1.0));
                let dw = nw * k;
                let dh = nh * k;
                let dx = notes_x + (inner_w - dw) * 0.5;
                let dy = notes_y + (notes_h - dh) * 0.5;
                let _ = writeln!(
                    content,
                    "q {dw:.2} 0 0 {dh:.2} {dx:.2} {dy:.2} cm /ImN Do Q"
                );
                let _ = n_img_id;
            } else {
                let title_y = notes_y + notes_h - 18.0;
                let _ = writeln!(
                    content,
                    "q 0.18 0.50 0.72 rg BT /F2 8.5 Tf {:.2} {title_y:.2} Td (SPEAKER NOTES) Tj ET Q",
                    notes_x + 12.0
                );
                if let Some(wrapped) = wrapped_notes {
                    let mut ty = title_y - 16.0;
                    let max_lines = (((ty - (notes_y + 10.0)) / 14.5).floor() as usize) + 1;
                    let truncated = wrapped.len() > max_lines;
                    let visible = wrapped.len().min(max_lines);
                    let _ = writeln!(content, "q 0.10 0.13 0.17 rg BT /F1 10.5 Tf");
                    for (line_idx, mut line) in wrapped.into_iter().take(visible).enumerate() {
                        if ty < notes_y + 10.0 {
                            break;
                        }
                        if truncated && line_idx + 1 == visible {
                            let chars: Vec<char> = line.chars().collect();
                            if chars.len() > 3 {
                                line = chars[..chars.len() - 3].iter().collect();
                            }
                            line.push_str("...");
                        }
                        let _ = writeln!(
                            content,
                            "1 0 0 1 {:.2} {ty:.2} Tm ({}) Tj",
                            notes_x + 12.0,
                            pdf_escape_winansi(&line)
                        );
                        ty -= 14.5;
                    }
                    let _ = writeln!(content, "ET Q");
                } else {
                    let ty = title_y - 18.0;
                    let _ = writeln!(
                        content,
                        "q 0.52 0.56 0.61 rg BT /F1 9.5 Tf {:.2} {ty:.2} Td (No speaker notes for this slide.) Tj ET Q",
                        notes_x + 12.0
                    );
                }
            }

            (out_w, out_h, (slide_x, slide_y, sw, sh))
        };

        let (sx_pdf, sy_pdf, sw_pdf, sh_pdf) = slide_rect_pdf;
        // Draw the slide image XObject.
        let _ = writeln!(
            content,
            "q {sw_pdf:.2} 0 0 {sh_pdf:.2} {sx_pdf:.2} {sy_pdf:.2} cm /ImS Do Q"
        );
        if include_notes {
            let _ = writeln!(
                content,
                "q 0.80 0.83 0.86 RG 0.6 w {sx_pdf:.2} {sy_pdf:.2} {sw_pdf:.2} {sh_pdf:.2} re S Q"
            );
        }

        // Overlay every freehand/highlighter annotation stroke as crisp native PDF vector paths.
        write_pdf_strokes(&mut content, strokes, sx_pdf, sy_pdf, sw_pdf, sh_pdf);

        let stream_id = writer.add_stream(content.as_bytes());
        let mut xobjects = format!("/ImS {slide_img_id} 0 R");
        if let Some((n_img_id, _, _)) = beamer_notes_img {
            let _ = write!(xobjects, " /ImN {n_img_id} 0 R");
        }
        let page_dict = format!(
            "<< /Type /Page /Parent {pages_root_id} 0 R /MediaBox [0 0 {out_w:.2} {out_h:.2}] \
             /Resources << /Font << /F1 {font_id} 0 R /F2 {font_bold_id} 0 R >> \
             /ExtGState << /GSOp {gs_opaque_id} 0 R /GSHl {gs_highlight_id} 0 R >> \
             /XObject << {xobjects} >> >> \
             /Contents {stream_id} 0 R >>"
        );
        page_ids.push(writer.add_object(page_dict.into_bytes()));

        push_whiteboards(
            slide_num,
            slide_num == pdf_total,
            &mut display_num,
            &mut writer,
            &mut page_ids,
        );
    }

    let kids: String = page_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let pages_dict = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        page_ids.len(),
        kids
    );
    writer.set_reserved(pages_root_id, pages_dict.into_bytes());
    let catalog_id =
        writer.add_object(format!("<< /Type /Catalog /Pages {pages_root_id} 0 R >>").into_bytes());

    Ok(writer.finish(catalog_id))
}

fn write_pdf_strokes(
    content: &mut String,
    strokes: &[StrokePath],
    sx_pdf: f32,
    sy_pdf: f32,
    sw_pdf: f32,
    sh_pdf: f32,
) {
    if strokes.is_empty() {
        return;
    }
    let _ = writeln!(
        content,
        "q {sx_pdf:.2} {sy_pdf:.2} {sw_pdf:.2} {sh_pdf:.2} re W n 1 J 1 j"
    );
    for stroke in strokes {
        if stroke.points.is_empty() {
            continue;
        }
        let (r, g, b) = stroke.color.rgb_f32();
        let line_w = (stroke.width * sh_pdf).clamp(0.8, 42.0);
        let gs = if stroke.highlighter { "/GSHl" } else { "/GSOp" };
        let _ = writeln!(content, "{gs} gs {r:.3} {g:.3} {b:.3} RG {line_w:.2} w");
        let p0 = stroke.points[0];
        let x0 = sx_pdf + p0[0].clamp(0.0, 1.0) * sw_pdf;
        let y0 = sy_pdf + (1.0 - p0[1].clamp(0.0, 1.0)) * sh_pdf;
        if stroke.points.len() == 1 {
            let _ = writeln!(content, "{x0:.2} {y0:.2} m {:.2} {y0:.2} l S", x0 + 0.2);
        } else {
            let _ = write!(content, "{x0:.2} {y0:.2} m");
            for pt in &stroke.points[1..] {
                let x = sx_pdf + pt[0].clamp(0.0, 1.0) * sw_pdf;
                let y = sy_pdf + (1.0 - pt[1].clamp(0.0, 1.0)) * sh_pdf;
                let _ = write!(content, " {x:.2} {y:.2} l");
            }
            let _ = writeln!(content, " S");
        }
    }
    let _ = writeln!(content, "Q");
}

#[allow(clippy::too_many_arguments)]
fn emit_whiteboard_page(
    writer: &mut PdfWriter,
    pages_root_id: usize,
    font_id: usize,
    font_bold_id: usize,
    gs_opaque_id: usize,
    gs_highlight_id: usize,
    wb: &WhiteboardSlide,
    sw: f32,
    sh: f32,
    display_num: usize,
    total: usize,
    include_notes: bool,
) -> usize {
    let mut content = String::with_capacity(1024);
    let (out_w, out_h, sx_pdf, sy_pdf) = if !include_notes {
        (sw, sh, 0.0f32, 0.0f32)
    } else {
        let margin = (sw * 0.055).clamp(18.0, 36.0);
        let header_h = 20.0f32;
        let out_w = sw + margin * 2.0;
        let out_h = margin + sh + header_h + margin;
        let _ = writeln!(
            content,
            "q 0.965 0.969 0.965 rg 0 0 {out_w:.2} {out_h:.2} re f Q"
        );
        let header_y = out_h - margin - 12.0;
        let _ = writeln!(
            content,
            "q 0.36 0.41 0.47 rg BT /F2 9.5 Tf {margin:.2} {header_y:.2} Td ({}) Tj ET Q",
            pdf_escape_winansi(&format!("Slide {display_num} / {total} · Whiteboard"))
        );
        (out_w, out_h, margin, margin)
    };

    // White paper board.
    let _ = writeln!(
        content,
        "q 1 1 1 rg {sx_pdf:.2} {sy_pdf:.2} {sw:.2} {sh:.2} re f Q"
    );
    if wb.grid {
        let step = (sh / 14.0).clamp(12.0, 28.0);
        let _ = writeln!(content, "q 0.87 0.90 0.93 RG 0.4 w");
        let mut gx = sx_pdf + step;
        while gx < sx_pdf + sw - 1.0 {
            let _ = writeln!(
                content,
                "{gx:.2} {sy_pdf:.2} m {gx:.2} {:.2} l S",
                sy_pdf + sh
            );
            gx += step;
        }
        let mut gy = sy_pdf + step;
        while gy < sy_pdf + sh - 1.0 {
            let _ = writeln!(
                content,
                "{sx_pdf:.2} {gy:.2} m {:.2} {gy:.2} l S",
                sx_pdf + sw
            );
            gy += step;
        }
        let _ = writeln!(content, "Q");
    }
    if include_notes {
        let _ = writeln!(
            content,
            "q 0.80 0.83 0.86 RG 0.6 w {sx_pdf:.2} {sy_pdf:.2} {sw:.2} {sh:.2} re S Q"
        );
    }

    write_pdf_strokes(&mut content, &wb.strokes, sx_pdf, sy_pdf, sw, sh);

    let stream_id = writer.add_stream(content.as_bytes());
    let page_dict = format!(
        "<< /Type /Page /Parent {pages_root_id} 0 R /MediaBox [0 0 {out_w:.2} {out_h:.2}] \
         /Resources << /Font << /F1 {font_id} 0 R /F2 {font_bold_id} 0 R >> \
         /ExtGState << /GSOp {gs_opaque_id} 0 R /GSHl {gs_highlight_id} 0 R >> >> \
         /Contents {stream_id} 0 R >>"
    );
    writer.add_object(page_dict.into_bytes())
}

fn crop_to_jpeg(
    rgba: &[u8],
    full_w: usize,
    full_h: usize,
    x_frac: f32,
    w_frac: f32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let x0 = ((x_frac * full_w as f32).round() as usize).min(full_w.saturating_sub(1));
    let crop_w = ((w_frac * full_w as f32).round() as usize)
        .max(1)
        .min(full_w - x0);
    let mut rgb = Vec::with_capacity(crop_w * full_h * 3);
    for y in 0..full_h {
        let row = y * full_w;
        for x in x0..(x0 + crop_w) {
            let off = (row + x) * 4;
            // hayro renders premultiplied RGBA over opaque white background.
            rgb.push(rgba[off]);
            rgb.push(rgba[off + 1]);
            rgb.push(rgba[off + 2]);
        }
    }
    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88);
    encoder
        .write_image(&rgb, crop_w as u32, full_h as u32, ColorType::Rgb8.into())
        .map_err(|e| format!("Could not encode slide image: {e}"))?;
    Ok((out, crop_w as u32, full_h as u32))
}

/// Simple word wrapper for Type1 Helvetica (avg glyph width ~0.52 * font_size).
/// Splits single unbreakable tokens longer than `max_chars` (such as URLs) so text never
/// overflows the notes card or PDF MediaBox.
fn wrap_pdf_lines(text: &str, max_width: f32, font_size: f32) -> Vec<String> {
    let max_chars = ((max_width.max(0.0) / (font_size * 0.52)).floor() as usize).max(8);
    let mut out = Vec::new();
    for para in text.lines() {
        let trimmed = para.trim_end();
        if trimmed.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in trimmed.split_whitespace() {
            let word_len = word.chars().count();
            if word_len > max_chars {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                let mut chunk = String::new();
                for ch in word.chars() {
                    chunk.push(ch);
                    if chunk.chars().count() >= max_chars {
                        out.push(std::mem::take(&mut chunk));
                    }
                }
                if !chunk.is_empty() {
                    line = chunk;
                }
                continue;
            }
            if line.is_empty() {
                line.push_str(word);
            } else if line.chars().count() + 1 + word_len <= max_chars {
                line.push(' ');
                line.push_str(word);
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
    }
    out
}

/// Escapes a string into PDF literal string syntax encoded in WinAnsi (CP1252) with clean ASCII
/// fallbacks for common typographic characters.
fn pdf_escape_winansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let mapped = match ch {
            '—' | '–' => '-',
            '“' | '”' => '"',
            '‘' | '’' => '\'',
            '…' => {
                out.push_str("...");
                continue;
            }
            '•' => '*',
            '→' => {
                out.push_str("->");
                continue;
            }
            '←' => {
                out.push_str("<-");
                continue;
            }
            c if (c as u32) < 0x20 => ' ',
            c if (c as u32) <= 0x7e => c,
            c if (0xa0..=0xff).contains(&(c as u32)) => c,
            _ => '?',
        };
        match mapped {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) <= 0x7e => out.push(c),
            c => {
                let _ = write!(out, "\\{:03o}", c as u32);
            }
        }
    }
    out
}

/// Minimal PDF 1.4 object writer with valid byte-offset xref table.
struct PdfWriter {
    objects: Vec<Option<Vec<u8>>>,
}

impl PdfWriter {
    fn new() -> Self {
        Self {
            objects: Vec::new(),
        }
    }

    fn reserve_id(&mut self) -> usize {
        self.objects.push(None);
        self.objects.len()
    }

    fn set_reserved(&mut self, id: usize, body: Vec<u8>) {
        self.objects[id - 1] = Some(body);
    }

    fn add_object(&mut self, body: Vec<u8>) -> usize {
        self.objects.push(Some(body));
        self.objects.len()
    }

    fn add_stream(&mut self, data: &[u8]) -> usize {
        let mut obj = format!("<< /Length {} >>\nstream\n", data.len()).into_bytes();
        obj.extend_from_slice(data);
        obj.extend_from_slice(b"\nendstream");
        self.add_object(obj)
    }

    fn add_jpeg_xobject(&mut self, jpeg: &[u8], w: u32, h: u32) -> usize {
        let mut obj = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
             /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .into_bytes();
        obj.extend_from_slice(jpeg);
        obj.extend_from_slice(b"\nendstream");
        self.add_object(obj)
    }

    fn finish(self, catalog_id: usize) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(self.objects.len() + 1);
        offsets.push(0usize);
        for (i, obj) in self.objects.into_iter().enumerate() {
            offsets.push(out.len());
            let id = i + 1;
            let _ = writeln!(out, "{id} 0 obj");
            if let Some(body) = obj {
                out.extend_from_slice(&body);
            }
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_pos = out.len();
        let _ = writeln!(out, "xref\n0 {}", offsets.len());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets[1..] {
            let _ = writeln!(out, "{off:010} 00000 n ");
        }
        let _ = writeln!(
            out,
            "trailer\n<< /Size {} /Root {catalog_id} 0 R >>\nstartxref\n{xref_pos}\n%%EOF",
            offsets.len()
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::present::data::PenColor;

    #[test]
    fn exported_annotated_pdf_with_notes_reloads_in_hayro() {
        // Build a minimal 1-page valid PDF as input.
        let mut w = PdfWriter::new();
        let pages_id = w.reserve_id();
        let content_id = w.add_stream(b"q 0.9 0.9 0.9 rg 0 0 362 272 re f Q");
        let page_id = w.add_object(
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 362 272] /Contents {content_id} 0 R >>"
            )
            .into_bytes(),
        );
        w.set_reserved(
            pages_id,
            format!("<< /Type /Pages /Count 1 /Kids [{page_id} 0 R] >>").into_bytes(),
        );
        let cat = w.add_object(format!("<< /Type /Catalog /Pages {pages_id} 0 R >>").into_bytes());
        let src_pdf = w.finish(cat);

        let mut ann = BTreeMap::new();
        ann.insert(
            1,
            vec![
                StrokePath {
                    color: PenColor::Crimson,
                    width: 0.006,
                    highlighter: false,
                    points: vec![[0.1, 0.2], [0.5, 0.5], [0.8, 0.3]],
                },
                StrokePath {
                    color: PenColor::Amber,
                    width: 0.022,
                    highlighter: true,
                    points: vec![[0.2, 0.6], [0.7, 0.6]],
                },
            ],
        );
        let mut notes = BTreeMap::new();
        notes.insert(1, "Key proof idea — emphasize convergence rate!".into());
        let whiteboards = vec![WhiteboardSlide {
            id: 1,
            after_pdf_page: 1,
            grid: true,
            strokes: vec![StrokePath {
                color: PenColor::Emerald,
                width: 0.006,
                highlighter: false,
                points: vec![[0.2, 0.2], [0.8, 0.8]],
            }],
        }];

        // 1. Slides only (1 PDF page + 1 inserted whiteboard = 2 pages)
        let slides_only =
            export_annotated_pdf(&src_pdf, &ann, &notes, &whiteboards, false, false).unwrap();
        let parsed_a = Pdf::new(Arc::new(slides_only)).unwrap();
        assert_eq!(parsed_a.pages().len(), 2);
        let (wa, ha) = parsed_a.pages()[0].render_dimensions();
        assert!((wa - 362.0).abs() < 1.0 && (ha - 272.0).abs() < 1.0);

        // 2. Slides + speaker notes handout
        let with_notes =
            export_annotated_pdf(&src_pdf, &ann, &notes, &whiteboards, true, false).unwrap();
        let parsed_b = Pdf::new(Arc::new(with_notes)).unwrap();
        assert_eq!(parsed_b.pages().len(), 2);
        let (wb, hb) = parsed_b.pages()[0].render_dimensions();
        assert!(
            wb > 362.0 && hb > 272.0,
            "handout page should expand for notes"
        );
    }

    #[test]
    fn wrap_pdf_lines_splits_long_urls_and_export_truncates_overflow_with_ellipsis() {
        let url = "https://example.org/".to_owned() + &"segment".repeat(25);
        let lines = wrap_pdf_lines(&url, 180.0, 10.5);
        assert!(lines.len() >= 4, "long unbreakable URL must be split");
        assert!(lines.iter().all(|l| l.chars().count() <= 36));

        // Build a 1-page PDF and pass 80 lines of notes so the notes card hits max height.
        let mut w = PdfWriter::new();
        let pages_id = w.reserve_id();
        let content_id = w.add_stream(b"q 1 1 1 rg 0 0 362 272 re f Q");
        let page_id = w.add_object(
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 362 272] /Contents {content_id} 0 R >>"
            )
            .into_bytes(),
        );
        w.set_reserved(
            pages_id,
            format!("<< /Type /Pages /Count 1 /Kids [{page_id} 0 R] >>").into_bytes(),
        );
        let cat = w.add_object(format!("<< /Type /Catalog /Pages {pages_id} 0 R >>").into_bytes());
        let src_pdf = w.finish(cat);

        let mut notes = BTreeMap::new();
        let long_note = (1..=80)
            .map(|i| format!("Line {i}: talking point details"))
            .collect::<Vec<_>>()
            .join("\n");
        notes.insert(1, long_note);
        let out =
            export_annotated_pdf(&src_pdf, &BTreeMap::new(), &notes, &[], true, false).unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.contains("...) Tj"),
            "truncated handout notes should end with an ellipsis"
        );
        assert!(Pdf::new(Arc::new(out)).is_ok());
    }
}
