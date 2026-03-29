use anyhow::Context;
use chrono::{DateTime, NaiveDate, Utc};
use rrule::RRuleSet;
use serde_json::{json, Value};

use crate::date::{canonical_date, get_date_part};

#[derive(Debug, Clone)]
pub struct RecurrenceInput {
    pub recurrence: String,
    pub recurrence_anchor: String,
    pub scheduled: Option<String>,
    pub due: Option<String>,
    pub date_created: Option<String>,
    pub complete_instances: Vec<String>,
    pub skipped_instances: Vec<String>,
}

fn parse_rrule_set(rule: &str, source_date: &str) -> anyhow::Result<RRuleSet> {
    let dt = if source_date.contains('T') {
        DateTime::parse_from_rfc3339(source_date)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                let date = NaiveDate::parse_from_str(&get_date_part(source_date), "%Y-%m-%d")?;
                Ok::<_, anyhow::Error>(DateTime::<Utc>::from_naive_utc_and_offset(
                    date.and_hms_opt(0, 0, 0).expect("valid midnight"),
                    Utc,
                ))
            })?
    } else {
        let date = NaiveDate::parse_from_str(&get_date_part(source_date), "%Y-%m-%d")?;
        DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(0, 0, 0).expect("valid midnight"),
            Utc,
        )
    };

    let formatted = if rule.contains("DTSTART") {
        rule.replace(';', "\n")
    } else {
        format!("DTSTART:{}\nRRULE:{}", dt.format("%Y%m%dT%H%M%SZ"), rule)
    };
    formatted.parse::<RRuleSet>().context("invalid recurrence")
}

fn next_occurrence(rule: &str, source_date: &str, after: &str) -> anyhow::Result<Option<String>> {
    let after_date = NaiveDate::parse_from_str(&get_date_part(after), "%Y-%m-%d")?;
    let after_dt = DateTime::<Utc>::from_naive_utc_and_offset(
        after_date.and_hms_opt(0, 0, 0).expect("valid midnight"),
        Utc,
    );
    let set = parse_rrule_set(rule, source_date)?;
    for dt in set.into_iter().take(512) {
        let utc = dt.with_timezone(&Utc);
        let day = utc.format("%Y-%m-%d").to_string();
        if day >= after_dt.format("%Y-%m-%d").to_string() {
            return Ok(Some(day));
        }
    }
    Ok(None)
}

pub fn complete(input: &RecurrenceInput, completion_date: &str) -> anyhow::Result<Value> {
    let completion_date = canonical_date(completion_date)?;
    let mut complete_instances = input.complete_instances.clone();
    if !complete_instances
        .iter()
        .any(|entry| entry == &completion_date)
    {
        complete_instances.push(completion_date.clone());
    }
    let skipped_instances: Vec<String> = input
        .skipped_instances
        .iter()
        .filter(|entry| *entry != &completion_date)
        .cloned()
        .collect();

    let source_date = input
        .scheduled
        .as_deref()
        .or(input.date_created.as_deref())
        .unwrap_or(&completion_date);
    let next_scheduled = next_occurrence(&input.recurrence, source_date, &completion_date)?;

    let next_due = match (&input.due, &input.scheduled, &next_scheduled) {
        (Some(due), Some(scheduled), Some(next)) => {
            let due = NaiveDate::parse_from_str(&get_date_part(due), "%Y-%m-%d")?;
            let scheduled = NaiveDate::parse_from_str(&get_date_part(scheduled), "%Y-%m-%d")?;
            let next = NaiveDate::parse_from_str(next, "%Y-%m-%d")?;
            let offset = due.signed_duration_since(scheduled);
            Some(
                next.checked_add_signed(offset)
                    .map(|date| date.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| next.format("%Y-%m-%d").to_string()),
            )
        }
        _ => None,
    };

    Ok(json!({
        "updatedRecurrence": input.recurrence,
        "nextScheduled": next_scheduled,
        "nextDue": next_due,
        "completeInstances": complete_instances,
        "skippedInstances": skipped_instances,
    }))
}

pub fn recalculate(input: &RecurrenceInput, reference_date: &str) -> anyhow::Result<Value> {
    let source_date = input
        .scheduled
        .as_deref()
        .or(input.date_created.as_deref())
        .unwrap_or(reference_date);
    let next = next_occurrence(&input.recurrence, source_date, reference_date)?;
    Ok(json!({
        "updatedRecurrence": input.recurrence,
        "nextScheduled": next,
        "nextDue": Value::Null,
    }))
}

pub fn uncomplete_instance(instances: &[String], target: &str) -> Value {
    let target = get_date_part(target);
    let next: Vec<String> = instances
        .iter()
        .filter(|value| get_date_part(value) != target)
        .cloned()
        .collect();
    json!({ "completeInstances": next })
}

pub fn skip_instance(instances: &[String], target: &str) -> Value {
    let target = get_date_part(target);
    let mut next = instances.to_vec();
    if !next.iter().any(|value| get_date_part(value) == target) {
        next.push(target);
    }
    json!({ "skippedInstances": next })
}

pub fn unskip_instance(instances: &[String], target: &str) -> Value {
    let target = get_date_part(target);
    let next: Vec<String> = instances
        .iter()
        .filter(|value| get_date_part(value) != target)
        .cloned()
        .collect();
    json!({ "skippedInstances": next })
}

pub fn effective_state(completed: &[String], skipped: &[String], target: &str) -> Value {
    let target = get_date_part(target);
    let completed = completed.iter().any(|value| get_date_part(value) == target);
    let skipped = skipped.iter().any(|value| get_date_part(value) == target);
    let state = if completed {
        "completed"
    } else if skipped {
        "skipped"
    } else {
        "open"
    };
    json!({ "value": state })
}
