//! Human-facing formatting. The rules are deliberately boring: one decimal
//! below 100, no trailing zero fields in a duration, and a dash for unknown.

/// The unit ladder shared by every byte figure.
const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

/// A byte figure: `512 B`, `4.0 KiB`, `103 MiB`.
pub fn bytes(n: u64) -> String {
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A signed byte figure, for reclaimable totals. `i64::MIN` is handled without
/// negating it.
pub fn bytes_signed(n: i64) -> String {
    if n < 0 {
        let magnitude = n.unsigned_abs();
        format!("-{}", bytes(magnitude))
    } else {
        bytes(n as u64)
    }
}

/// Percentages keep one decimal below 100. CPU may exceed 100% on several
/// cores; that is correct rather than clamped.
pub fn percent(v: f64) -> String {
    if !v.is_finite() {
        return "—".to_owned();
    }
    if v >= 100.0 {
        format!("{v:.0}%")
    } else {
        format!("{v:.1}%")
    }
}

/// `12s`, `4m 12s`, `3h 04m`, `2d 06h`. Zero trailing fields are dropped so a
/// table never says `15h 00m`.
pub fn duration(seconds: i64) -> String {
    let s = seconds.max(0);
    if s < 60 {
        return format!("{s}s");
    }
    if s < 3600 {
        let rem = s % 60;
        return if rem == 0 {
            format!("{}m", s / 60)
        } else {
            format!("{}m {rem:02}s", s / 60)
        };
    }
    if s < 86_400 {
        let rem = (s % 3600) / 60;
        return if rem == 0 {
            format!("{}h", s / 3600)
        } else {
            format!("{}h {rem:02}m", s / 3600)
        };
    }
    let rem = (s % 86_400) / 3600;
    if rem == 0 {
        format!("{}d", s / 86_400)
    } else {
        format!("{}d {rem:02}h", s / 86_400)
    }
}

/// How long ago, from two Unix timestamps in seconds. Saturating subtraction
/// keeps an engine that reports a future time from producing a huge duration.
pub fn ago(unix_seconds: i64, now_seconds: i64) -> String {
    duration(now_seconds.saturating_sub(unix_seconds))
}

/// `1 image`, `2 images`.
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Seconds since the epoch.
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// `14:32:07`, from a Unix timestamp in local time.
pub fn clock(unix_seconds: i64) -> String {
    match chrono::DateTime::from_timestamp(unix_seconds, 0) {
        Some(stamp) => stamp
            .with_timezone(&chrono::Local)
            .format("%H:%M:%S")
            .to_string(),
        None => "—".to_owned(),
    }
}

/// `15:56` today, `Oct 28 15:56` otherwise.
pub fn started(unix_seconds: i64, now_seconds: i64) -> String {
    let Some(stamp) = chrono::DateTime::from_timestamp(unix_seconds, 0) else {
        return "—".to_owned();
    };
    let local = stamp.with_timezone(&chrono::Local);
    let Some(now) = chrono::DateTime::from_timestamp(now_seconds, 0) else {
        return local.format("%H:%M").to_string();
    };
    let now_local = now.with_timezone(&chrono::Local);
    if local.date_naive() == now_local.date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %d %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_the_ladder_and_one_decimal_below_a_hundred() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(4 * 1024), "4.0 KiB");
        assert_eq!(bytes(4 * 1024 * 1024), "4.0 MiB");
        assert_eq!(bytes(103 * 1024 * 1024), "103 MiB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }

    #[test]
    fn signed_bytes_keep_the_sign_and_never_overflow() {
        assert_eq!(bytes_signed(0), "0 B");
        assert_eq!(bytes_signed(-1), "-1 B");
        assert_eq!(bytes_signed(-1024), "-1.0 KiB");
        assert!(bytes_signed(i64::MIN).starts_with('-'));
    }

    #[test]
    fn percent_keeps_one_decimal_below_a_hundred() {
        assert_eq!(percent(0.0), "0.0%");
        assert_eq!(percent(0.04), "0.0%");
        assert_eq!(percent(12.34), "12.3%");
        assert_eq!(percent(99.9), "99.9%");
        assert_eq!(percent(100.0), "100%");
        assert_eq!(percent(109.0), "109%");
        assert_eq!(percent(f64::NAN), "—");
    }

    #[test]
    fn durations_drop_empty_trailing_fields() {
        assert_eq!(duration(0), "0s");
        assert_eq!(duration(12), "12s");
        assert_eq!(duration(60), "1m");
        assert_eq!(duration(72), "1m 12s");
        assert_eq!(duration(3600), "1h");
        assert_eq!(duration(3 * 3600 + 4 * 60), "3h 04m");
        assert_eq!(duration(86_400), "1d");
        assert_eq!(duration(2 * 86_400 + 6 * 3600), "2d 06h");
        assert_eq!(duration(-5), "0s");
    }

    #[test]
    fn ago_saturates_a_future_timestamp() {
        assert_eq!(ago(100, 160), "1m");
        assert_eq!(ago(200, 100), "0s");
    }

    #[test]
    fn counts_are_plural() {
        assert_eq!(count(0, "image", "images"), "0 images");
        assert_eq!(count(1, "image", "images"), "1 image");
        assert_eq!(count(2, "image", "images"), "2 images");
    }

    #[test]
    fn clocks_are_local_and_have_a_stable_shape() {
        let now = now();
        assert!(now > 1_700_000_000);
        let clock = clock(now);
        assert_eq!(clock.len(), 8);
        assert_eq!(clock.matches(':').count(), 2);
        let started_today = started(now, now);
        assert_eq!(started_today.len(), 5);
    }
}
