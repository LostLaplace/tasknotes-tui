use serde_json::{json, Map, Value};

fn parse_iso_instant(value: &str, code: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| anyhow::anyhow!(code.to_string()))
}

fn canonical_instant(value: chrono::DateTime<chrono::Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn normalize_and_validate_time_entries(entries_input: &[Value]) -> anyhow::Result<Vec<Map<String, Value>>> {
    let mut normalized = Vec::new();
    let mut active_count = 0;

    for raw_entry in entries_input {
        let object = raw_entry
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("invalid_time_entry"))?;
        let start_text = object
            .get("startTime")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing_time_entry_start"))?;
        let start = parse_iso_instant(start_text, "invalid_time_entry_start")?;

        let mut entry = Map::new();
        entry.insert(
            "startTime".into(),
            Value::String(canonical_instant(start)),
        );

        match object.get("endTime") {
            None | Some(Value::Null) => active_count += 1,
            Some(Value::String(end_text)) if end_text.trim().is_empty() => active_count += 1,
            Some(Value::String(end_text)) => {
                let end = parse_iso_instant(end_text, "invalid_time_entry_end")?;
                if end < start {
                    anyhow::bail!("invalid_time_range");
                }
                entry.insert("endTime".into(), Value::String(canonical_instant(end)));
            }
            Some(_) => anyhow::bail!("invalid_time_entry_end"),
        }

        normalized.push(entry);
    }

    if active_count > 1 {
        anyhow::bail!("multiple_active_time_entries");
    }

    Ok(normalized)
}

pub fn start(entries_input: &[Value], now: Option<&str>) -> anyhow::Result<Value> {
    let entries = normalize_and_validate_time_entries(entries_input)?;
    if entries.iter().any(|entry| !entry.contains_key("endTime")) {
        anyhow::bail!("time_tracking_already_active");
    }

    let now = match now {
        Some(value) => parse_iso_instant(value, "invalid_time_now")?,
        None => chrono::Utc::now(),
    };
    let now = canonical_instant(now);
    let mut next = entries
        .into_iter()
        .map(Value::Object)
        .collect::<Vec<_>>();
    next.push(json!({ "startTime": now }));
    Ok(json!({
        "value": next,
        "dateModified": now,
    }))
}

pub fn stop(entries_input: &[Value], now: Option<&str>) -> anyhow::Result<Value> {
    let mut entries = normalize_and_validate_time_entries(entries_input)?;
    let Some(active_index) = entries.iter().position(|entry| !entry.contains_key("endTime")) else {
        anyhow::bail!("no_active_time_entry");
    };

    let now = match now {
        Some(value) => parse_iso_instant(value, "invalid_time_now")?,
        None => chrono::Utc::now(),
    };
    let now_text = canonical_instant(now);
    let start = entries[active_index]
        .get("startTime")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing_time_entry_start"))?;
    if chrono::DateTime::parse_from_rfc3339(&now_text)?.with_timezone(&chrono::Utc)
        < parse_iso_instant(start, "invalid_time_entry_start")?
    {
        anyhow::bail!("invalid_time_range");
    }

    entries[active_index].insert("endTime".into(), Value::String(now_text.clone()));
    Ok(json!({
        "value": entries.into_iter().map(Value::Object).collect::<Vec<_>>(),
        "dateModified": now_text,
    }))
}

pub fn replace_entries(entries_input: &[Value], date_modified: Option<&str>) -> anyhow::Result<Value> {
    let entries = normalize_and_validate_time_entries(entries_input)?;
    let modified = match date_modified {
        Some(value) if !value.trim().is_empty() => canonical_instant(parse_iso_instant(value, "invalid_date_modified")?),
        _ => canonical_instant(chrono::Utc::now()),
    };
    Ok(json!({
        "value": entries.into_iter().map(Value::Object).collect::<Vec<_>>(),
        "dateModified": modified,
    }))
}

pub fn remove_entry(
    entries_input: &[Value],
    selector: &Value,
    date_modified: Option<&str>,
) -> anyhow::Result<Value> {
    let entries = normalize_and_validate_time_entries(entries_input)?;
    let index = selector
        .get("index")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if index < 0 || index as usize >= entries.len() {
        anyhow::bail!("time_entry_not_found");
    }
    let modified = match date_modified {
        Some(value) if !value.trim().is_empty() => canonical_instant(parse_iso_instant(value, "invalid_date_modified")?),
        _ => canonical_instant(chrono::Utc::now()),
    };
    Ok(json!({
        "value": entries
            .into_iter()
            .enumerate()
            .filter(|(entry_index, _)| *entry_index != index as usize)
            .map(|(_, entry)| Value::Object(entry))
            .collect::<Vec<_>>(),
        "dateModified": modified,
    }))
}

pub fn auto_stop_on_complete(
    auto_stop_on_complete: bool,
    is_completion_transition: bool,
    task_entries: &[Value],
) -> anyhow::Result<Value> {
    if !auto_stop_on_complete || !is_completion_transition {
        return Ok(json!({ "stopped": false }));
    }
    let entries = normalize_and_validate_time_entries(task_entries)?;
    let stopped = entries.iter().any(|entry| !entry.contains_key("endTime"));
    Ok(json!({ "stopped": stopped }))
}

fn minutes_between(start_iso: &str, end_iso: &str) -> anyhow::Result<i64> {
    let start = parse_iso_instant(start_iso, "invalid_time_entry_start")?;
    let end = parse_iso_instant(end_iso, "invalid_time_entry_end")?;
    Ok(((end - start).num_minutes()).max(0))
}

pub fn report_totals(entries_input: &[Value], now: Option<&str>) -> anyhow::Result<Value> {
    let entries = normalize_and_validate_time_entries(entries_input)?;
    let now = match now {
        Some(value) => canonical_instant(parse_iso_instant(value, "invalid_time_now")?),
        None => canonical_instant(chrono::Utc::now()),
    };

    let mut closed_minutes = 0i64;
    let mut active_minutes = 0i64;
    for entry in entries {
        let start = entry
            .get("startTime")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing_time_entry_start"))?;
        if let Some(end) = entry.get("endTime").and_then(Value::as_str) {
            closed_minutes += minutes_between(start, end)?;
        } else {
            active_minutes += minutes_between(start, &now)?;
        }
    }
    Ok(json!({
        "closed_minutes": closed_minutes,
        "live_minutes": closed_minutes + active_minutes,
    }))
}

pub fn has_active_entry(entries_input: &[Value]) -> anyhow::Result<bool> {
    let entries = normalize_and_validate_time_entries(entries_input)?;
    Ok(entries.iter().any(|entry| !entry.contains_key("endTime")))
}
