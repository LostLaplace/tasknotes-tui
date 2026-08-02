# Changelog

## v0.1.10 - 2026-08-02

- add an `isProject` expression variable: true when a task's own path is referenced as a
  project by some other task's `projects:` field (the project-as-task pattern), so
  expression views can filter project notes out of actionable-task lists, e.g.
  `where: "status == \"next_action\" && !isProject"`

## v0.1.9 - 2026-08-01

- date pickers now open cleared instead of pre-filled with today's date when a field has no existing value (creating a task, or editing due/scheduled on a task that doesn't have one set); the calendar cursor still starts on today, so `t` or any arrow key picks it in one keystroke
- fix a correctness bug where editing *any* task field (toggling complete, renaming, archiving, editing due/scheduled/priority/status, time tracking) could silently write schema-default values (e.g. `occurrence_materialization`, `recurrence_anchor`) into that task's frontmatter, even though they were never actually set — a gap in the v0.1.6 fix, which only covered task creation, not edits

## v0.1.8 - 2026-08-01

- add `Shift-N` in the Projects view to quick-create a task linked to the highlighted project, with status pre-set to the first configured `next_action_statuses` value
- add a project picker step to the multi-step "New Task" flow (`n`), listing existing projects with a `(none)` option and free-text entry for a new (phantom) project; falls back to the active project when left on `(none)`, matching quick create's existing behavior

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
