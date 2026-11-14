//! UTC milliseconds + local offset helpers. Store both; never local time alone.

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Current local UTC offset in minutes (e.g. +360 for UTC+6).
pub fn local_offset_min() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}
