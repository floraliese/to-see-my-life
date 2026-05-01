use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, LocalResult, NaiveDateTime, NaiveTime, TimeZone};

pub fn parse_duration_minutes(input: &str) -> Result<i64> {
    let value = input.trim().to_lowercase().replace(' ', "");
    if value.is_empty() {
        return Err(anyhow!("duration cannot be empty"));
    }

    if let Ok(minutes) = value.parse::<i64>() {
        return positive_minutes(minutes);
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
            'h' => total += parsed * 60,
            'm' => total += parsed,
            _ => return Err(anyhow!("invalid duration unit '{ch}' in {input}")),
        }
        number.clear();
    }

    if !number.is_empty() {
        total += number.parse::<i64>()?;
    }

    positive_minutes(total)
}

fn positive_minutes(minutes: i64) -> Result<i64> {
    if minutes <= 0 {
        return Err(anyhow!("duration must be positive"));
    }
    Ok(minutes)
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
    use super::parse_duration_minutes;

    #[test]
    fn parses_duration() {
        assert_eq!(parse_duration_minutes("30").unwrap(), 30);
        assert_eq!(parse_duration_minutes("30m").unwrap(), 30);
        assert_eq!(parse_duration_minutes("1h").unwrap(), 60);
        assert_eq!(parse_duration_minutes("1h30m").unwrap(), 90);
    }
}
