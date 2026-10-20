//! Dates as the web app showed them with `toLocaleString(undefined, { dateStyle: "medium",
//! timeStyle: "short" })`, in the computer's time zone.

use chrono::{Local, TimeZone};

/// "20 Oct 2026, 14:05" for milliseconds since 1970.
pub fn medium_ms(ms: f64) -> String {
    match Local.timestamp_millis_opt(ms as i64).single() {
        Some(t) => t.format("%-d %b %Y, %H:%M").to_string(),
        None => String::new(),
    }
}

/// The same for seconds since 1970.
pub fn medium_secs(secs: i64) -> String {
    medium_ms(secs as f64 * 1000.0)
}

/// "14:05:09", for "Finished at".
pub fn time_ms(ms: f64) -> String {
    match Local.timestamp_millis_opt(ms as i64).single() {
        Some(t) => t.format("%H:%M:%S").to_string(),
        None => String::new(),
    }
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}
