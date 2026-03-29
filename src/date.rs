use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::json;

pub fn parse_date_to_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    let trimmed = value.trim();
    anyhow::ensure!(!trimmed.is_empty(), "Date string cannot be empty");

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(0, 0, 0).expect("valid midnight"),
            Utc,
        ));
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }

    anyhow::bail!("Invalid date \"{}\".", value);
}

pub fn parse_date_to_local(value: &str) -> anyhow::Result<DateTime<Local>> {
    let trimmed = value.trim();
    anyhow::ensure!(!trimmed.is_empty(), "Date string cannot be empty");

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let dt = Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("valid midnight"))
            .single()
            .ok_or_else(|| anyhow::anyhow!("Invalid date \"{}\".", value))?;
        return Ok(dt);
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Local));
    }

    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        let dt = Local
            .from_local_datetime(&dt)
            .single()
            .ok_or_else(|| anyhow::anyhow!("Invalid date \"{}\".", value))?;
        return Ok(dt);
    }

    anyhow::bail!("Invalid date \"{}\".", value);
}

pub fn validate_date_string(value: &str) -> anyhow::Result<String> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("Invalid date \"{}\". Expected YYYY-MM-DD.", value))?;
    Ok(value.trim().to_string())
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
    value.contains('T')
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
) -> String {
    if let Some(date) = explicit_date.and_then(|v| validate_date_string(v).ok()) {
        return date;
    }
    if let Some(date) = scheduled
        .map(get_date_part)
        .filter(|v| validate_date_string(v).is_ok())
    {
        return date;
    }
    if let Some(date) = due
        .map(get_date_part)
        .filter(|v| validate_date_string(v).is_ok())
    {
        return date;
    }
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn day_in_timezone(now: &str, timezone: &str) -> anyhow::Result<String> {
    let tz: Tz = timezone.parse()?;
    let instant = parse_date_to_utc(now)?;
    Ok(instant.with_timezone(&tz).format("%Y-%m-%d").to_string())
}

pub fn parse_utc_result(value: &str) -> anyhow::Result<serde_json::Value> {
    let parsed = parse_date_to_utc(value)?;
    Ok(json!({
        "date": parsed.format("%Y-%m-%d").to_string(),
        "isoDate": parsed.to_rfc3339(),
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
    first_of_target.with_day(day).map(|value| value.format("%Y-%m-%d").to_string())
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
