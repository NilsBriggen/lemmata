//! Project images for hover cards, read and decoded on a thread of their own; until one is ready
//! nothing shows, and the UI is asked to draw again when it is.

use eframe::egui::{self, ColorImage, TextureHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

enum Slot {
    Loading(Arc<Mutex<Option<Option<ColorImage>>>>),
    Ready(Option<TextureHandle>),
}

thread_local! {
    static CACHE: std::cell::RefCell<HashMap<PathBuf, Slot>> = Default::default();
}

/// A card shows at most 240 points: past this a photo's pixels only cost memory (12 MP is 48 MB).
const MAX_SIDE: usize = 1024;

/// The image scaled down to `MAX_SIDE`, keeping its proportions.
fn shrink(img: ColorImage) -> ColorImage {
    let [w, h] = img.size;
    if w.max(h) <= MAX_SIDE {
        return img;
    }
    let k = MAX_SIDE as f32 / w.max(h) as f32;
    let (nw, nh) = (
        ((w as f32 * k) as u32).max(1),
        ((h as f32 * k) as u32).max(1),
    );
    let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    let Some(src) = image::RgbaImage::from_raw(w as u32, h as u32, raw) else {
        return img;
    };
    let small = image::imageops::thumbnail(&src, nw, nh);
    ColorImage::from_rgba_premultiplied([nw as usize, nh as usize], small.as_raw())
}

/// The image's texture once it is decoded; None while loading or if it can't be read.
pub fn get(ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
    CACHE.with_borrow_mut(|cache| {
        if cache.len() > 64 {
            cache.retain(|_, s| matches!(s, Slot::Loading(_)));
        }
        let slot = cache.entry(path.to_owned()).or_insert_with(|| {
            let cell = Arc::new(Mutex::new(None));
            let (c, p, ctx) = (cell.clone(), path.to_owned(), ctx.clone());
            std::thread::spawn(move || {
                let img = std::fs::read(&p)
                    .ok()
                    .and_then(|b| crate::workspace::viewer::decode(&p.to_string_lossy(), &b).ok())
                    .map(shrink);
                *c.lock().unwrap() = Some(img);
                ctx.request_repaint();
            });
            Slot::Loading(cell)
        });
        if let Slot::Loading(cell) = slot {
            let done = cell.lock().unwrap().take();
            match done {
                Some(img) => {
                    *slot = Slot::Ready(img.map(|i| {
                        ctx.load_texture(path.to_string_lossy(), i, egui::TextureOptions::LINEAR)
                    }))
                }
                None => return None,
            }
        }
        match slot {
            Slot::Ready(t) => t.clone(),
            Slot::Loading(_) => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_images_are_held_at_card_size() {
        let big = ColorImage::filled([3000, 1000], egui::Color32::RED);
        let small = shrink(big);
        assert_eq!(small.size, [1024, 341]);
        assert_eq!(small.pixels[0], egui::Color32::RED);
        let icon = ColorImage::filled([64, 64], egui::Color32::BLUE);
        assert_eq!(shrink(icon).size, [64, 64]);
    }
}
