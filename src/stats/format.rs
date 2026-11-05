//! Turns statistics into the text panels show. `None` always becomes N/A.

use super::Power;

pub const NOT_AVAILABLE: &str = "N/A";

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn uptime(seconds: Option<u64>) -> String {
    let Some(seconds) = seconds else { return NOT_AVAILABLE.to_owned() };
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3_600 % 24, seconds / 60 % 60);
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}")
    } else {
        format!("{hours:02}:{minutes:02}")
    }
}

pub fn power(power: Option<Power>) -> String {
    match power {
        None => NOT_AVAILABLE.to_owned(),
        Some(Power::Ac) => "AC".to_owned(),
        Some(Power::Battery { percent: Some(percent), charging: true }) => format!("{percent}%+"),
        Some(Power::Battery { percent: Some(percent), charging: false }) => format!("{percent}%"),
        Some(Power::Battery { percent: None, .. }) => "BATT".to_owned(),
    }
}

pub fn temperature(celsius: Option<f32>) -> String {
    celsius.map_or_else(|| NOT_AVAILABLE.to_owned(), |c| format!("{c:.0}°C"))
}

pub fn percent(value: Option<f32>) -> String {
    value.map_or_else(|| NOT_AVAILABLE.to_owned(), |v| format!("{v:.0}%"))
}

/// Current and maximum clock in GHz, such as `2.1/4.6`.
pub fn frequency(current_mhz: Option<u64>, max_mhz: Option<u64>) -> String {
    if current_mhz.unwrap_or(0) == 0 && max_mhz.unwrap_or(0) == 0 {
        return NOT_AVAILABLE.to_owned();
    }
    let ghz = |mhz: Option<u64>| {
        mhz.filter(|m| *m > 0)
            .map_or_else(|| NOT_AVAILABLE.to_owned(), |m| format!("{:.1}", m as f64 / 1000.0))
    };
    format!("{}/{}", ghz(current_mhz), ghz(max_mhz))
}

/// `used` of `total` bytes, such as `2.1/3.9 GiB`.
pub fn gib_pair(used: u64, total: u64) -> String {
    format!("{:.1}/{:.1} GiB", used as f64 / GIB, total as f64 / GIB)
}

pub fn count(value: Option<usize>) -> String {
    value.map_or_else(|| NOT_AVAILABLE.to_owned(), |v| v.to_string())
}

pub fn text(value: Option<&str>) -> &str {
    value.unwrap_or(NOT_AVAILABLE)
}

/// Shorten to `max` characters, marking the cut.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_values_read_na() {
        assert_eq!(uptime(None), "N/A");
        assert_eq!(power(None), "N/A");
        assert_eq!(temperature(None), "N/A");
        assert_eq!(percent(None), "N/A");
        assert_eq!(count(None), "N/A");
        assert_eq!(text(None), "N/A");
        assert_eq!(frequency(None, None), "N/A");
        assert_eq!(frequency(Some(0), None), "N/A");
        assert_eq!(frequency(Some(2900), None), "2.9/N/A");
        assert_eq!(frequency(Some(0), Some(4600)), "N/A/4.6");
    }

    #[test]
    fn uptime_gains_days_when_needed() {
        assert_eq!(uptime(Some(59)), "00:00");
        assert_eq!(uptime(Some(3_660)), "01:01");
        assert_eq!(uptime(Some(90_061)), "1d 01:01");
    }

    #[test]
    fn power_states_are_distinct() {
        assert_eq!(power(Some(Power::Ac)), "AC");
        assert_eq!(power(Some(Power::Battery { percent: Some(84), charging: false })), "84%");
        assert_eq!(power(Some(Power::Battery { percent: Some(84), charging: true })), "84%+");
        assert_eq!(power(Some(Power::Battery { percent: None, charging: false })), "BATT");
    }

    #[test]
    fn numbers_read_cleanly() {
        assert_eq!(temperature(Some(51.6)), "52°C");
        assert_eq!(frequency(Some(2100), Some(4600)), "2.1/4.6");
        assert_eq!(gib_pair(2 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024), "2.0/4.0 GiB");
    }

    #[test]
    fn truncate_counts_characters_not_bytes() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly-10", 10), "exactly-10");
        assert_eq!(truncate("much longer text", 8), "much lo…");
        assert_eq!(truncate("日本語のテキスト", 4), "日本語…");
        assert_eq!(truncate("x", 0), "…");
    }
}
