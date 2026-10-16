//! Touchpad pinches on Linux. WebKitGTK would magnify the whole window, sidebars and editor too;
//! instead they go to the page as "pinch" events, and the PDF view zooms.

use gtk::gdk;
use tauri::{Emitter, Manager};

#[derive(Clone, serde::Serialize)]
struct Pinch {
    phase: &'static str,
    /// Relative to the start of the gesture; only updates carry one.
    scale: f64,
    x: f64,
    y: f64,
}

pub fn forward(app: &tauri::AppHandle) {
    use gtk::prelude::*;
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let target = window.clone();
    let _ = window.with_webview(move |webview| {
        let gesture = gtk::GestureZoom::new(&webview.inner());
        // Capture phase, and claimed: WebKit's own pinch handling never sees the gesture.
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        let send = move |g: &gtk::GestureZoom, phase: &'static str, scale: f64| {
            let (x, y) = g.bounding_box_center().unwrap_or((0.0, 0.0));
            let _ = target.emit_to(target.label(), "pinch", Pinch { phase, scale, x, y });
        };
        let s = send.clone();
        gesture.connect_begin(move |g, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            s(g, "begin", 1.0);
        });
        let s = send.clone();
        gesture.connect_scale_changed(move |g, scale| s(g, "update", scale));
        let s = send.clone();
        gesture.connect_end(move |g, _| s(g, "end", 1.0));
        gesture.connect_cancel(move |g, _| send(g, "cancel", 1.0));
        // GTK 3 widgets don't own their gestures; this one lives as long as the window.
        std::mem::forget(gesture);
    });
}

/// Test builds: a touchpad pinch made up and sent through GTK, where a real one comes in.
/// `phase`: 0 begin, 1 update, 2 end. `x`, `y`: in the webview.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn automation_pinch(
    window: tauri::WebviewWindow,
    phase: i8,
    scale: f64,
    x: f64,
    y: f64,
) -> Result<(), String> {
    use gtk::glib::translate::{ToGlibPtr, ToGlibPtrMut};
    use gtk::prelude::*;
    window
        .with_webview(move |webview| {
            let view = webview.inner();
            let (Some(target), Some(seat)) = (
                view.window(),
                gdk::Display::default().and_then(|d| d.default_seat()),
            ) else {
                return;
            };
            let mut event = gdk::Event::new(gdk::EventType::TouchpadPinch);
            event.set_device(seat.pointer().as_ref());
            // SAFETY: a TouchpadPinch event is a GdkEventTouchpadPinch; the event owns the window reference.
            unsafe {
                let ptr: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
                let raw = ptr as *mut gdk::ffi::GdkEventTouchpadPinch;
                (*raw).window = target.to_glib_full();
                (*raw).phase = phase;
                (*raw).n_fingers = 2;
                (*raw).scale = scale;
                (*raw).x = x;
                (*raw).y = y;
            }
            gtk::main_do_event(&mut event);
        })
        .map_err(|e| e.to_string())
}
