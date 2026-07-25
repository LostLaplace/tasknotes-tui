use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::{days_between, is_before_date_safe};
use crate::repository::{project_links, TaskRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UrgencyConfig {
    pub due: f64,
    pub due_ramp_days: f64,
    pub scheduled: f64,
    pub active: f64,
    pub age: f64,
    pub age_max_days: f64,
    pub project: f64,
    pub priority: BTreeMap<String, f64>,
    pub tags: BTreeMap<String, f64>,
}

impl Default for UrgencyConfig {
    fn default() -> Self {
        Self {
            due: 12.0,
            due_ramp_days: 14.0,
            scheduled: 5.0,
            active: 4.0,
            age: 2.0,
            age_max_days: 365.0,
            project: 1.0,
            priority: BTreeMap::from([
                ("urgent".to_string(), 9.0),
                ("high".to_string(), 6.0),
                ("normal".to_string(), 1.0),
                ("low".to_string(), -3.0),
            ]),
            tags: BTreeMap::from([("next".to_string(), 15.0)]),
        }
    }
}

/// A single named term in an urgency calculation, in the spirit of Taskwarrior's
/// `task <id> info` breakdown: `contribution = coefficient * value`.
#[derive(Debug, Clone, PartialEq)]
pub struct UrgencyTerm {
    pub label: String,
    pub coefficient: f64,
    pub value: f64,
    pub contribution: f64,
}

fn term(label: impl Into<String>, coefficient: f64, value: f64) -> UrgencyTerm {
    UrgencyTerm {
        label: label.into(),
        coefficient,
        value,
        contribution: coefficient * value,
    }
}

/// Per-term breakdown of the urgency calculation, in the order Taskwarrior-style reports
/// present them. Only terms whose underlying data is present on `task` are included.
pub fn compute_urgency_breakdown(
    task: &TaskRecord,
    today: &str,
    config: &UrgencyConfig,
) -> Vec<UrgencyTerm> {
    let mut terms = Vec::new();

    if let Some(due) = task.due.as_deref() {
        if let Some(days_until) = days_between(today, due) {
            let ramp = ((config.due_ramp_days - days_until as f64) / config.due_ramp_days)
                .clamp(0.0, 1.0);
            terms.push(term("due", config.due, ramp));
        }
    }

    if let Some(scheduled) = task.scheduled.as_deref() {
        let value = if !is_before_date_safe(today, scheduled) {
            1.0
        } else {
            0.0
        };
        terms.push(term("scheduled", config.scheduled, value));
    }

    if let Some(created) = task
        .normalized_frontmatter
        .get("dateCreated")
        .and_then(|value| value.as_str())
    {
        if let Some(age_days) = days_between(created, today) {
            let ramp = (age_days as f64 / config.age_max_days).clamp(0.0, 1.0);
            terms.push(term("age", config.age, ramp));
        }
    }

    if let Some(priority) = task.priority.as_deref() {
        if let Some(coefficient) = config.priority.get(priority).copied() {
            terms.push(term(format!("priority ({priority})"), coefficient, 1.0));
        }
    }

    if task.has_active_time_entry {
        terms.push(term("active", config.active, 1.0));
    }

    if let Some(tags) = task
        .normalized_frontmatter
        .get("tags")
        .and_then(|value| value.as_array())
    {
        for tag in tags.iter().filter_map(|value| value.as_str()) {
            if let Some(coefficient) = config.tags.get(tag).copied() {
                terms.push(term(format!("tag ({tag})"), coefficient, 1.0));
            }
        }
    }

    if !project_links(task).is_empty() {
        terms.push(term("project", config.project, 1.0));
    }

    terms
}

/// Weighted sum of urgency terms, Taskwarrior-style. Higher means more urgent.
/// Callers are responsible for excluding completed/archived tasks where that matters —
/// this is a pure function of the fields present on `task`.
pub fn compute_urgency(task: &TaskRecord, today: &str, config: &UrgencyConfig) -> f64 {
    compute_urgency_breakdown(task, today, config)
        .iter()
        .map(|term| term.contribution)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn base_task() -> TaskRecord {
        TaskRecord {
            path: "task.md".into(),
            title: "Task".into(),
            status: "open".into(),
            priority: None,
            due: None,
            scheduled: None,
            time_entries: Vec::new(),
            has_active_time_entry: false,
            body: String::new(),
            normalized_frontmatter: Map::new(),
            raw_frontmatter: Map::new(),
        }
    }

    #[test]
    fn overdue_task_outranks_far_future_task() {
        let config = UrgencyConfig::default();
        let today = "2026-01-15";

        let mut overdue = base_task();
        overdue.due = Some("2026-01-01".into());

        let mut future = base_task();
        future.due = Some("2026-06-01".into());

        assert!(
            compute_urgency(&overdue, today, &config) > compute_urgency(&future, today, &config)
        );
    }

    #[test]
    fn urgent_priority_outranks_low_priority_at_equal_due_date() {
        let config = UrgencyConfig::default();
        let today = "2026-01-15";

        let mut urgent = base_task();
        urgent.due = Some(today.to_string());
        urgent.priority = Some("urgent".into());

        let mut low = base_task();
        low.due = Some(today.to_string());
        low.priority = Some("low".into());

        assert!(compute_urgency(&urgent, today, &config) > compute_urgency(&low, today, &config));
    }

    #[test]
    fn active_time_tracking_boosts_score() {
        let config = UrgencyConfig::default();
        let today = "2026-01-15";

        let mut active = base_task();
        active.has_active_time_entry = true;

        let idle = base_task();

        assert!(compute_urgency(&active, today, &config) > compute_urgency(&idle, today, &config));
    }

    #[test]
    fn age_term_saturates_at_age_max_days() {
        let config = UrgencyConfig::default();
        let today = "2026-01-15";

        let mut old = base_task();
        old.normalized_frontmatter
            .insert("dateCreated".into(), json!("2020-01-01"));

        let mut ancient = base_task();
        ancient
            .normalized_frontmatter
            .insert("dateCreated".into(), json!("1990-01-01"));

        assert_eq!(
            compute_urgency(&old, today, &config),
            compute_urgency(&ancient, today, &config)
        );
    }

    #[test]
    fn breakdown_terms_sum_to_total_and_label_priority() {
        let config = UrgencyConfig::default();
        let today = "2026-01-15";

        let mut task = base_task();
        task.due = Some(today.to_string());
        task.priority = Some("high".into());
        task.has_active_time_entry = true;

        let breakdown = compute_urgency_breakdown(&task, today, &config);
        let total: f64 = breakdown.iter().map(|term| term.contribution).sum();

        assert_eq!(total, compute_urgency(&task, today, &config));
        assert!(breakdown.iter().any(|term| term.label == "priority (high)"));
        assert!(breakdown.iter().any(|term| term.label == "active"));
        assert!(breakdown.iter().any(|term| term.label == "due"));
    }

    #[test]
    fn tags_and_project_add_configured_bonus() {
        let mut config = UrgencyConfig::default();
        config.tags.insert("hot".into(), 3.0);
        let today = "2026-01-15";

        let mut tagged = base_task();
        tagged
            .normalized_frontmatter
            .insert("tags".into(), json!(["hot"]));

        let plain = base_task();

        assert_eq!(
            compute_urgency(&tagged, today, &config) - compute_urgency(&plain, today, &config),
            3.0
        );
    }
}
