// util — 時間解析與格式化工具函數。
// 提供統一的時間字符串解析入口，被 todo 和 timer 模塊調用。
// 支持的 duration 格式：純數字（分鐘）、30m、1h、1h30m、10s（主要用於測試）。
// 支持的 start time 格式：now、HH:MM、HH.MM、YYYY-MM-DD HH:MM。
// 所有解析錯誤都返回可讀的錯誤信息，不 panic。

use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, LocalResult, NaiveDateTime, NaiveTime, TimeZone};

pub fn parse_duration_minutes(input: &str) -> Result<i64> {
    let seconds = parse_duration_seconds(input)?;
    Ok((seconds + 59) / 60)
}

pub fn parse_duration_seconds(input: &str) -> Result<i64> {
    let value = input.trim().to_lowercase().replace(' ', "");
    if value.is_empty() {
        return Err(anyhow!("duration cannot be empty"));
    }

    if let Ok(minutes) = value.parse::<i64>() {
        return positive_seconds(minutes * 60);
    }

    let mut total = 0_i64;
    let mut number = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }

        if number.is_empty() {
            return Err(anyhow!("invalid duration: {input}"));
        }

        let parsed = number.parse::<i64>()?;
        match ch {
            'h' => total += parsed * 3600,
            'm' => total += parsed * 60,
            's' => total += parsed,
            _ => return Err(anyhow!("invalid duration unit '{ch}' in {input}")),
        }
        number.clear();
    }

    if !number.is_empty() {
        total += number.parse::<i64>()? * 60;
    }

    positive_seconds(total)
}

fn positive_seconds(seconds: i64) -> Result<i64> {
    if seconds <= 0 {
        return Err(anyhow!("duration must be positive"));
    }
    Ok(seconds)
}

pub fn parse_start_time(input: &str, now: DateTime<Local>) -> Result<DateTime<Local>> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("now") {
        return Ok(now);
    }

    let normalized_time = trimmed.replace('.', ":");
    if let Ok(time) = NaiveTime::parse_from_str(&normalized_time, "%H:%M") {
        let date = now.date_naive();
        let naive = NaiveDateTime::new(date, time);
        return localize(naive);
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
        return localize(naive);
    }

    Err(anyhow!(
        "invalid start time. Use HH:MM, HH.MM, 'now', or YYYY-MM-DD HH:MM"
    ))
}

fn localize(naive: NaiveDateTime) -> Result<DateTime<Local>> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(value) => Ok(value),
        LocalResult::Ambiguous(first, _) => Ok(first),
        LocalResult::None => Err(anyhow!("the selected local time does not exist")),
    }
}

pub fn format_minutes(minutes: i64) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 {
        format!("{hours}h{mins:02}m")
    } else {
        format!("{mins}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_duration_seconds ──

    #[test]
    fn parses_seconds() {
        assert_eq!(parse_duration_seconds("30").unwrap(), 30 * 60);
        assert_eq!(parse_duration_seconds("30m").unwrap(), 30 * 60);
        assert_eq!(parse_duration_seconds("1h").unwrap(), 3600);
        assert_eq!(parse_duration_seconds("1h30m").unwrap(), 5400);
        assert_eq!(parse_duration_seconds("10s").unwrap(), 10);
        assert_eq!(parse_duration_seconds("1m30s").unwrap(), 90);
        assert_eq!(parse_duration_seconds("5s").unwrap(), 5);
    }

    #[test]
    fn rejects_empty_duration() {
        assert!(parse_duration_seconds("").is_err());
        assert!(parse_duration_seconds(" ").is_err());
    }

    #[test]
    fn rejects_zero_or_negative() {
        assert!(parse_duration_seconds("0").is_err());
        assert!(parse_duration_seconds("0m").is_err());
        assert!(parse_duration_seconds("-5m").is_err());
        assert!(parse_duration_seconds("0s").is_err());
    }

    #[test]
    fn rejects_invalid_units() {
        assert!(parse_duration_seconds("abc").is_err());
        assert!(parse_duration_seconds("5x").is_err());
        assert!(parse_duration_seconds("1d").is_err());
    }

    // ── parse_duration_minutes (wraps seconds, rounds up) ──

    #[test]
    fn parses_duration_minutes() {
        assert_eq!(parse_duration_minutes("30").unwrap(), 30);
        assert_eq!(parse_duration_minutes("30m").unwrap(), 30);
        assert_eq!(parse_duration_minutes("1h").unwrap(), 60);
        assert_eq!(parse_duration_minutes("1h30m").unwrap(), 90);
        assert_eq!(parse_duration_minutes("10s").unwrap(), 1);
        assert_eq!(parse_duration_minutes("90s").unwrap(), 2);
    }

    // ── parse_start_time ──

    #[test]
    fn parses_now() {
        let now = Local::now();
        let result = parse_start_time("now", now).unwrap();
        // within 1 second
        let diff = (result - now).num_seconds().abs();
        assert!(diff <= 1, "now diff was {diff}s");
    }

    #[test]
    fn parses_hhmm() {
        let now = Local::now();
        let result = parse_start_time("14:30", now).unwrap();
        assert_eq!(result.format("%H:%M").to_string(), "14:30");
        assert_eq!(result.date_naive(), now.date_naive());
    }

    #[test]
    fn parses_hh_dot_mm() {
        let now = Local::now();
        let result = parse_start_time("14.30", now).unwrap();
        assert_eq!(result.format("%H:%M").to_string(), "14:30");
    }

    #[test]
    fn rejects_invalid_start() {
        let now = Local::now();
        assert!(parse_start_time("nope", now).is_err());
        assert!(parse_start_time("", now).is_err());
    }

    // ── format_minutes ──

    #[test]
    fn formats_minutes() {
        assert_eq!(format_minutes(5), "5m");
        assert_eq!(format_minutes(60), "1h00m");
        assert_eq!(format_minutes(90), "1h30m");
        assert_eq!(format_minutes(0), "0m");
    }
}
