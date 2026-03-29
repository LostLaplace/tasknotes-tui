use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use regex::Regex;
use serde_json::json;

fn strict_date_regex() -> Regex {
    Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid regex")
}

fn strict_datetime_regex() -> Regex {
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?(?:Z|[+-]\d{2}:\d{2})?$")
        .expect("valid regex")
}

fn is_strict_datetime(trimmed: &str) -> bool {
    if !strict_datetime_regex().is_match(trimmed) {
        return false;
    }

    let hour = trimmed.get(11..13).and_then(|v| v.parse::<u32>().ok());
    let minute = trimmed.get(14..16).and_then(|v| v.parse::<u32>().ok());
    let second = trimmed.get(17..19).and_then(|v| v.parse::<u32>().ok());

    matches!(hour, Some(0..=23)) && matches!(minute, Some(0..=59)) && matches!(second, Some(0..=59))
}

pub fn parse_date_to_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    let trimmed = value.trim();
    anyhow::ensure!(!trimmed.is_empty(), "Date string cannot be empty");

    if strict_date_regex().is_match(trimmed) {
        let date = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Invalid date \"{}\".", value))?;
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(0, 0, 0).expect("valid midnight"),
            Utc,
        ));
    }

    if is_strict_datetime(trimmed) {
        if trimmed.ends_with('Z')
            || trimmed
                .as_bytes()
                .iter()
                .rev()
                .take(6)
                .any(|byte| *byte == b'+' || *byte == b'-')
        {
            let dt = DateTime::parse_from_rfc3339(trimmed)
                .map_err(|_| anyhow::anyhow!("Invalid date \"{}\".", value))?;
            return Ok(dt.with_timezone(&Utc));
        }

        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f") {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
        }
    }

    anyhow::bail!("Invalid date \"{}\".", value);
}

pub fn parse_date_to_local(value: &str) -> anyhow::Result<DateTime<Local>> {
    let trimmed = value.trim();
    anyhow::ensure!(!trimmed.is_empty(), "Date string cannot be empty");

    if strict_date_regex().is_match(trimmed) {
        let date = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Invalid date \"{}\".", value))?;
        let dt = Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("valid midnight"))
            .single()
            .ok_or_else(|| anyhow::anyhow!("Invalid date \"{}\".", value))?;
        return Ok(dt);
    }

    if is_strict_datetime(trimmed) {
        if trimmed.ends_with('Z')
            || trimmed
                .as_bytes()
                .iter()
                .rev()
                .take(6)
                .any(|byte| *byte == b'+' || *byte == b'-')
        {
            let dt = DateTime::parse_from_rfc3339(trimmed)
                .map_err(|_| anyhow::anyhow!("Invalid date \"{}\".", value))?;
            return Ok(dt.with_timezone(&Local));
        }

        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f") {
            let dt = Local
                .from_local_datetime(&dt)
                .single()
                .ok_or_else(|| anyhow::anyhow!("Invalid date \"{}\".", value))?;
            return Ok(dt);
        }
    }

    anyhow::bail!("Invalid date \"{}\".", value);
}

pub fn validate_date_string(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    anyhow::ensure!(
        strict_date_regex().is_match(trimmed),
        "Invalid date \"{}\". Expected YYYY-MM-DD.",
        value
    );
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("Invalid date \"{}\". Expected YYYY-MM-DD.", value))?;
    Ok(trimmed.to_string())
}

pub fn get_date_part(value: &str) -> String {
    if value.len() >= 10
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
    {
        return value[..10].to_string();
    }
    parse_date_to_utc(value)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

pub fn has_time_component(value: &str) -> bool {
    Regex::new(r"T\d{2}:\d{2}")
        .expect("valid regex")
        .is_match(value.trim())
}

pub fn is_same_date_safe(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && get_date_part(a) == get_date_part(b)
}

pub fn is_before_date_safe(a: &str, b: &str) -> bool {
    let a = NaiveDate::parse_from_str(&get_date_part(a), "%Y-%m-%d");
    let b = NaiveDate::parse_from_str(&get_date_part(b), "%Y-%m-%d");
    match (a, b) {
        (Ok(a), Ok(b)) => a < b,
        _ => false,
    }
}

pub fn resolve_operation_target_date(
    explicit_date: Option<&str>,
    scheduled: Option<&str>,
    due: Option<&str>,
) -> anyhow::Result<String> {
    if let Some(date) = explicit_date {
        return validate_date_string(date);
    }
    if let Some(date) = scheduled
        .map(get_date_part)
        .filter(|v| validate_date_string(v).is_ok())
    {
        return Ok(date);
    }
    if let Some(date) = due
        .map(get_date_part)
        .filter(|v| validate_date_string(v).is_ok())
    {
        return Ok(date);
    }
    Ok(Local::now().format("%Y-%m-%d").to_string())
}

pub fn day_in_timezone(instant: &str, timezone: &str) -> anyhow::Result<String> {
    let tz: Tz = timezone.parse()?;
    let instant = parse_date_to_utc(instant)?;
    Ok(instant.with_timezone(&tz).format("%Y-%m-%d").to_string())
}

pub fn parse_utc_result(value: &str) -> anyhow::Result<serde_json::Value> {
    let parsed = parse_date_to_utc(value)?;
    Ok(json!({
        "date": parsed.format("%Y-%m-%d").to_string(),
        "isoDate": parsed.to_rfc3339().get(..10).unwrap_or_default(),
    }))
}

pub fn parse_local_result(value: &str) -> anyhow::Result<serde_json::Value> {
    let parsed = parse_date_to_local(value)?;
    Ok(json!({
        "localDate": parsed.format("%Y-%m-%d").to_string(),
        "isoDate": parsed.with_timezone(&Utc).format("%Y-%m-%d").to_string(),
    }))
}

pub fn canonical_date(value: &str) -> anyhow::Result<String> {
    let dt = parse_date_to_utc(value)?;
    Ok(dt.format("%Y-%m-%d").to_string())
}

pub fn today_local() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn apply_day_offset(base: &str, offset_days: i64) -> Option<String> {
    let date = NaiveDate::parse_from_str(&get_date_part(base), "%Y-%m-%d").ok()?;
    Some(
        date.checked_add_signed(chrono::Duration::days(offset_days))?
            .format("%Y-%m-%d")
            .to_string(),
    )
}

pub fn apply_month_offset(base: &str, offset_months: i32) -> Option<String> {
    let date = NaiveDate::parse_from_str(&get_date_part(base), "%Y-%m-%d").ok()?;
    let mut year = date.year();
    let mut month = date.month() as i32 + offset_months;

    while month < 1 {
        month += 12;
        year -= 1;
    }
    while month > 12 {
        month -= 12;
        year += 1;
    }

    let first_of_target = NaiveDate::from_ymd_opt(year, month as u32, 1)?;
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month as u32 + 1, 1)?
    };
    let max_day = next_month.pred_opt()?.day();
    let day = date.day().min(max_day);
    first_of_target
        .with_day(day)
        .map(|value| value.format("%Y-%m-%d").to_string())
}

pub fn compare_day(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let a = NaiveDate::parse_from_str(&get_date_part(a), "%Y-%m-%d").ok()?;
    let b = NaiveDate::parse_from_str(&get_date_part(b), "%Y-%m-%d").ok()?;
    Some(a.cmp(&b))
}

pub fn year_month_day(value: &str) -> Option<(i32, u32, u32)> {
    let parsed = NaiveDate::parse_from_str(&get_date_part(value), "%Y-%m-%d").ok()?;
    Some((parsed.year(), parsed.month(), parsed.day()))
}
