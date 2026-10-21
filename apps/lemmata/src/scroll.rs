//! Touchpad scrolling that coasts on after the fingers lift, as the WebKitGTK app did.
//!
//! On Linux, winit passes touchpad scrolling as point deltas with start/move/end phases but
//! nothing after the fingers lift, and egui-winit can't tell a touchpad from a wheel there. This
//! marks those scrolls as trackpad ones and adds the momentum phase macOS sends by itself, so
//! egui's scroll physics (rubber band at the ends included) treat them the same way.

use eframe::egui::{self, Event, MouseWheelSource, MouseWheelUnit, TouchPhase, Vec2};

/// Finger scrolling moves the content a little further than the fingers, like GTK's.
const SPEED: f32 = 1.4;
/// How far back the speed at lift-off is measured.
const WINDOW: f64 = 0.08;
/// Seconds for the coast to slow to 1/e of its speed.
const DECAY: f32 = 0.33;
/// Points per second below which a coast doesn't start, or ends.
const MIN_SPEED: f32 = 40.0;

#[derive(Default)]
pub struct Momentum {
    /// Recent finger deltas with their time, for the speed at lift-off.
    recent: Vec<(f64, Vec2)>,
    /// Points per second while coasting.
    coast: Option<Vec2>,
    last: f64,
    /// A finger scroll ended: egui drops what is still unapplied on End, so it is sent a frame late.
    ending: bool,
}

impl Momentum {
    pub fn filter(&mut self, raw: &mut egui::RawInput, ctx: &egui::Context) {
        let now = raw.time.unwrap_or(self.last);
        let mut out = Vec::with_capacity(raw.events.len() + 2);
        if std::mem::take(&mut self.ending) {
            out.push(wheel(
                Vec2::ZERO,
                TouchPhase::End,
                MouseWheelSource::Trackpad,
            ));
        }
        for e in raw.events.drain(..) {
            match e {
                Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta,
                    phase,
                    source: MouseWheelSource::Unknown | MouseWheelSource::Trackpad,
                    modifiers,
                } => match phase {
                    TouchPhase::Start => {
                        self.stop(&mut out);
                        self.recent.clear();
                        out.push(Event::MouseWheel {
                            unit: MouseWheelUnit::Point,
                            delta,
                            phase,
                            source: MouseWheelSource::Trackpad,
                            modifiers,
                        });
                    }
                    TouchPhase::Move => {
                        let d = delta * SPEED;
                        self.recent.push((now, d));
                        out.push(Event::MouseWheel {
                            unit: MouseWheelUnit::Point,
                            delta: d,
                            phase,
                            source: MouseWheelSource::Trackpad,
                            modifiers,
                        });
                    }
                    TouchPhase::End | TouchPhase::Cancel => {
                        self.recent.retain(|(t, _)| now - t <= WINDOW);
                        let speed =
                            self.recent.iter().fold(Vec2::ZERO, |a, (_, d)| a + *d) / WINDOW as f32;
                        self.recent.clear();
                        // Ctrl+scroll zooms; that doesn't coast.
                        if phase == TouchPhase::End
                            && !modifiers.command
                            && speed.length() > MIN_SPEED
                        {
                            self.coast = Some(speed);
                            self.last = now;
                            out.push(wheel(
                                Vec2::ZERO,
                                TouchPhase::Start,
                                MouseWheelSource::Momentum,
                            ));
                        } else {
                            self.ending = true;
                            ctx.request_repaint();
                        }
                    }
                },
                Event::PointerButton { pressed: true, .. } | Event::Key { pressed: true, .. } => {
                    self.stop(&mut out);
                    out.push(e);
                }
                e => out.push(e),
            }
        }
        if let Some(v) = self.coast {
            let dt = (now - self.last).clamp(0.0, 0.05) as f32;
            self.last = now;
            let v = v * (-dt / DECAY).exp();
            if v.length() < MIN_SPEED {
                self.stop(&mut out);
            } else {
                if dt > 0.0 {
                    out.push(wheel(v * dt, TouchPhase::Move, MouseWheelSource::Momentum));
                }
                self.coast = Some(v);
                ctx.request_repaint();
            }
        }
        raw.events = out;
    }

    fn stop(&mut self, out: &mut Vec<Event>) {
        if self.coast.take().is_some() {
            out.push(wheel(
                Vec2::ZERO,
                TouchPhase::End,
                MouseWheelSource::Momentum,
            ));
        }
    }
}

fn wheel(delta: Vec2, phase: TouchPhase, source: MouseWheelSource) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta,
        phase,
        source,
        modifiers: egui::Modifiers::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(m: &mut Momentum, ctx: &egui::Context, time: f64, events: Vec<Event>) -> Vec<Event> {
        let mut raw = egui::RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        m.filter(&mut raw, ctx);
        raw.events
    }

    fn finger(dy: f32, phase: TouchPhase) -> Event {
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: Vec2::new(0.0, dy),
            phase,
            source: MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn moved(events: &[Event], source: MouseWheelSource) -> f32 {
        events
            .iter()
            .filter_map(|e| match e {
                Event::MouseWheel {
                    delta,
                    phase: TouchPhase::Move,
                    source: s,
                    ..
                } if *s == source => Some(delta.y),
                _ => None,
            })
            .sum()
    }

    #[test]
    fn a_flick_coasts_and_slows_down() {
        let (mut m, ctx) = (Momentum::default(), egui::Context::default());
        frame(&mut m, &ctx, 0.0, vec![finger(0.0, TouchPhase::Start)]);
        let mut t = 0.0;
        for _ in 0..5 {
            t += 1.0 / 60.0;
            let out = frame(&mut m, &ctx, t, vec![finger(-10.0, TouchPhase::Move)]);
            assert_eq!(
                moved(&out, MouseWheelSource::Trackpad),
                -10.0 * SPEED,
                "finger scrolls a bit further"
            );
        }
        t += 1.0 / 60.0;
        frame(&mut m, &ctx, t, vec![finger(0.0, TouchPhase::End)]);
        let mut steps = Vec::new();
        for _ in 0..200 {
            t += 1.0 / 60.0;
            let out = frame(&mut m, &ctx, t, Vec::new());
            let d = moved(&out, MouseWheelSource::Momentum);
            if d == 0.0 {
                break;
            }
            steps.push(d);
        }
        assert!(steps.len() > 10, "it coasts: {steps:?}");
        assert!(
            steps.windows(2).all(|w| w[1].abs() < w[0].abs()),
            "and slows down"
        );
        assert!(steps.iter().all(|d| *d < 0.0), "in the same direction");
        assert!(m.coast.is_none(), "then stops");
    }

    #[test]
    fn a_key_or_click_stops_the_coast_and_a_slow_lift_doesnt_start_one() {
        let (mut m, ctx) = (Momentum::default(), egui::Context::default());
        frame(&mut m, &ctx, 0.0, vec![finger(0.0, TouchPhase::Start)]);
        frame(&mut m, &ctx, 0.016, vec![finger(-20.0, TouchPhase::Move)]);
        frame(&mut m, &ctx, 0.032, vec![finger(0.0, TouchPhase::End)]);
        assert!(m.coast.is_some());
        let key = Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut m, &ctx, 0.048, vec![key]);
        assert!(m.coast.is_none());

        // Fingers that stopped moving before lifting: no coast, and the End comes a frame late.
        frame(&mut m, &ctx, 1.0, vec![finger(0.0, TouchPhase::Start)]);
        frame(&mut m, &ctx, 1.016, vec![finger(-20.0, TouchPhase::Move)]);
        let out = frame(&mut m, &ctx, 1.5, vec![finger(0.0, TouchPhase::End)]);
        assert!(m.coast.is_none() && out.is_empty());
        let out = frame(&mut m, &ctx, 1.516, Vec::new());
        assert!(matches!(
            out[..],
            [Event::MouseWheel {
                phase: TouchPhase::End,
                ..
            }]
        ));
    }

    #[test]
    fn mouse_wheels_pass_unchanged() {
        let (mut m, ctx) = (Momentum::default(), egui::Context::default());
        let notch = Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: Vec2::new(0.0, -1.0),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        };
        let out = frame(&mut m, &ctx, 0.0, vec![notch.clone()]);
        assert_eq!(out, vec![notch]);
    }
}
