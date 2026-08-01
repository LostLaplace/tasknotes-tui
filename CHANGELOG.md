# Changelog

## v0.1.7 - 2026-08-01

- add an opt-in Projects view (`kind: "projects"`) that lists every distinct project referenced by tasks' `projects:` links, including projects with no backing note, with open/total task counts and an earliest due/scheduled date
- add `Enter`/`Esc` drill-down navigation into and out of a project's linked tasks
- add a configurable `next_action_statuses` next-action indicator for project rows

## v0.1.6 - 2026-08-01

- stop stamping a redundant `type: task` frontmatter property on created/updated tasks (type is already identified via the task type's match rule, e.g. the `task` tag)
- stop writing optional schema-default properties (e.g. `occurrence_materialization`, `occurrence_next_trigger`, `recurrence_anchor`) into tasks that never set them
- quick create (`c`) no longer defaults new tasks to a scheduled date

## v0.1.5 - 2026-07-31

- fix task creation always resolving to the same path (`task.md`) when `title.storage: filename` is set, causing every task after the first to fail with `path_conflict`

## v0.1.4 - 2026-07-27

- add Taskwarrior-style urgency scoring with configurable coefficients (due proximity, scheduled date, priority, age, active time-tracking, tags, project membership)
- add an opt-in `sort: urgency` per-view key and an `urgency` expression variable for custom views
- show a Taskwarrior-style urgency term breakdown in the detail pane
- fix clearing due/scheduled/priority fields not persisting to disk

## v0.1.3 - 2026-03-31

- add active project context, including `Shift-P` toggle behavior and project-aware quick create
- add a default Project view and project-scoped expression variables
- resolve task `projects` links for project filtering, including wikilink and markdown-link cases
- preserve focused date when switching back to the Date view
- surface the active project in the State pane
- refresh the README to document the current defaults and project workflow
