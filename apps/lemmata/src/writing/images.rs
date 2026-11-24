//! Project images for hover cards, read and decoded on a thread of their own; until one is ready
//! nothing shows, and the UI is asked to draw again when it is.

use eframe::egui::{self, ColorImage, TextureHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The file's modified time when it was read: a changed one (a script wrote the plot again, or
/// the file appeared) reads it again.
type Stamp = Option<std::time::SystemTime>;

enum Slot {
    Loading(Arc<Mutex<Option<Option<ColorImage>>>>, Stamp),
    Ready(Option<TextureHandle>, Stamp),
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
    let stamp: Stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    CACHE.with_borrow_mut(|cache| {
        if cache.len() > 64 {
            cache.retain(|_, s| matches!(s, Slot::Loading(..)));
        }
        if let Some(Slot::Ready(_, then)) = cache.get(path)
            && *then != stamp
        {
            cache.remove(path);
        }
        let slot = cache.entry(path.to_owned()).or_insert_with(|| {
            let cell = Arc::new(Mutex::new(None));
            let (c, p, ctx) = (cell.clone(), path.to_owned(), ctx.clone());
            std::thread::spawn(move || {
                let img = lemmata_env::fs::read(&p)
                    .ok()
                    .and_then(|b| crate::workspace::viewer::decode(&p.to_string_lossy(), &b).ok())
                    .map(shrink);
                *c.lock().unwrap() = Some(img);
                ctx.request_repaint();
            });
            Slot::Loading(cell, stamp)
        });
        if let Slot::Loading(cell, then) = slot {
            let done = cell.lock().unwrap().take();
            match done {
                Some(img) => {
                    let then = *then;
                    *slot = Slot::Ready(
                        img.map(|i| {
                            ctx.load_texture(
                                path.to_string_lossy(),
                                i,
                                egui::TextureOptions::LINEAR,
                            )
                        }),
                        then,
                    )
                }
                None => return None,
            }
        }
        match slot {
            Slot::Ready(t, _) => t.clone(),
            Slot::Loading(..) => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_written_after_a_failed_read_or_changed_is_read_again() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("plot.png");
        let ctx = egui::Context::default();
        let wait = |want: Option<[usize; 2]>| {
            let start = std::time::Instant::now();
            loop {
                let got = get(&ctx, &f).map(|t| t.size());
                if got == want {
                    return;
                }
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(5),
                    "{got:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        assert!(get(&ctx, &f).is_none());
        std::thread::sleep(std::time::Duration::from_millis(50));
        let png = |w: u32| image::RgbaImage::new(w, 2).save(&f).unwrap();
        png(4);
        wait(Some([4, 2]));
        std::thread::sleep(std::time::Duration::from_millis(1100));
        png(6);
        wait(Some([6, 2]));
    }

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
