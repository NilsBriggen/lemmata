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
                    .and_then(|b| crate::workspace::viewer::decode(&p.to_string_lossy(), &b).ok());
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
