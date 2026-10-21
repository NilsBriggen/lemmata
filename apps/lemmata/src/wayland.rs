//! Touchpad pinches on Wayland. winit 0.30 doesn't bind `zwp_pointer_gestures_v1`, so this does,
//! on winit's own connection with a queue of its own, and hands each step to egui as a zoom.
//! X11 has no pinch here; Ctrl+scroll zooms there.

use eframe::egui;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_pointer::WlPointer, wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle, backend::Backend, delegate_noop};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_pinch_v1::{
    self as pinch, ZwpPointerGesturePinchV1,
};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gestures_v1::ZwpPointerGesturesV1;

/// The pinch steps, and the thread reading them.
pub struct Pinches {
    rx: Receiver<f32>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Pinches {
    /// Appends the pinch steps since the last frame as zoom events.
    pub fn drain_into(&self, events: &mut Vec<egui::Event>) {
        events.extend(self.rx.try_iter().map(egui::Event::Zoom));
    }

    /// Before winit closes the display: the thread reads from it.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct State {
    tx: Sender<f32>,
    ctx: egui::Context,
    /// The pinch's scale so far: Wayland sends it relative to where the pinch began.
    scale: f64,
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ignore WlPointer);
delegate_noop!(State: ZwpPointerGesturesV1);

impl Dispatch<ZwpPointerGesturePinchV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwpPointerGesturePinchV1,
        event: pinch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            pinch::Event::Begin { .. } => state.scale = 1.0,
            pinch::Event::Update { scale, .. } => {
                let step = scale / state.scale;
                state.scale = scale;
                if step.is_finite() && step > 0.0 && step != 1.0 {
                    let _ = state.tx.send(step as f32);
                    state.ctx.request_repaint();
                }
            }
            _ => {}
        }
    }
}

/// Starts listening for pinches on the display eframe runs on, if it is Wayland's and the
/// compositor offers gestures.
pub fn listen(
    display: &dyn raw_window_handle::HasDisplayHandle,
    ctx: &egui::Context,
) -> Option<Pinches> {
    let raw_window_handle::RawDisplayHandle::Wayland(h) = display.display_handle().ok()?.as_raw()
    else {
        return None;
    };
    // SAFETY: winit's display lives until the event loop ends; `Pinches::stop`, called from
    // `on_exit` before that, ends the only user of this backend.
    let backend = unsafe { Backend::from_foreign_display(h.display.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);
    let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
    let qh = queue.handle();
    let gestures: ZwpPointerGesturesV1 = globals.bind(&qh, 1..=3, ()).ok()?;
    let seat: WlSeat = globals.bind(&qh, 1..=7, ()).ok()?;
    let pointer = seat.get_pointer(&qh, ());
    let gesture = gestures.get_pinch_gesture(&pointer, &qh, ());
    let (tx, rx) = channel();
    let mut state = State {
        tx,
        ctx: ctx.clone(),
        scale: 1.0,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let thread = std::thread::Builder::new()
        .name("pinch".into())
        .spawn(move || {
            let _objects = (gestures, seat, pointer, gesture);
            // Wakes every 100 ms to see whether to stop, rather than blocking on the socket.
            while !stopped.load(Ordering::Relaxed) {
                if queue.dispatch_pending(&mut state).is_err() || conn.flush().is_err() {
                    break;
                }
                let Some(guard) = queue.prepare_read() else {
                    continue;
                };
                let mut fd = libc::pollfd {
                    fd: guard.connection_fd().as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: one valid pollfd; a dropped guard cancels the read.
                if unsafe { libc::poll(&mut fd, 1, 100) } > 0 && guard.read().is_err() {
                    break;
                }
            }
        })
        .ok()?;
    Some(Pinches {
        rx,
        stop,
        thread: Some(thread),
    })
}
