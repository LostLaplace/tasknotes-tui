# tasknotes-tui

A terminal interface for managing Markdown-based tasks. Built in Rust on top of
[mdbase-rs](https://github.com/callumalpass/mdbase-rs) and compatible with
TaskNotes collections.

Tasks live as markdown files with YAML frontmatter in your vault. The TUI reads and writes them directly — no database, no sync service.

## Install

From source:

```bash
cargo install --git <repo-url> --bin tasknotes-tui
```

Prebuilt binaries for Linux x86_64, macOS (Intel and Apple Silicon), and Windows are attached to GitHub Releases.

## Setup

Your vault needs an `mdbase.yaml` and a task type definition. The TUI runs
against mdbase-rs v0.3 and continues to read v0.2 collections, so existing
TaskNotes vaults do not need an in-place rewrite just to use it:

```bash
tasknotes-tui --root /path/to/vault
```

The TUI reads two config files from the vault root:

- **`tasknotes.yaml`** — TaskNotes spec config: field mapping, defaults, archive behavior, status values.
- **`tasknotes-tui.yaml`** (optional) — TUI-specific config: keybinds and view slots.

To see the default TUI config:

```bash
tasknotes-tui print-default-config
```

## Usage

### Views

Number keys `1`–`9` switch between configured view slots. The defaults:

| Key | View | Shows |
|-----|------|-------|
| `1` | Open | Open tasks |
| `2` | Date | Tasks for the focused date |
| `3` | Overdue | Past-due tasks |
| `4` | All | All non-archived tasks |
| `5` | Tracked | Tasks with an active timer |
| `6` | Archived | Archived tasks |
| `7` | Project | Tasks linked to the active project |

### Calendar

A mini calendar sits in the top pane. The highlighted day is the focused date, and days with tasks are marked.

- `h` / `l` or arrow keys — move by day
- `PgUp` / `PgDn` — move by week
- `g` — jump to today

When the date view (`2`) is active, the task list shows tasks for the focused date.

### Working with tasks

| Key | Action |
|-----|--------|
| `x` or `Space` | Toggle completion |
| `z` | Archive / restore |
| `T` | Start / stop time tracking |
| `S` | Skip / unskip a recurring instance |
| `P` | Toggle active project on selected task |
| `n` | Create task (multi-step: title, dates, priority, status, recurrence) |
| `c` | Quick create (title only, scheduled to focused date, linked to active project if set) |
| `e` | Edit title |
| `i` | Open in `$EDITOR` |
| `d` | Edit due date |
| `s` | Edit scheduled date |
| `p` | Edit priority |
| `t` | Edit status |
| `R` | Edit recurrence rule |
| `A` | Edit recurrence anchor |

### Date picker

When editing due or scheduled dates, a date picker opens:

- Arrow keys or `h`/`l` — move by day
- `j`/`k` — move by week
- `H`/`L` — move by month
- `t` — jump to today
- `c` — clear the value
- `/` — switch to manual `YYYY-MM-DD` entry
- `Enter` — save

### Command palette

`Ctrl-P` opens a fuzzy-filterable command palette with all available actions.

### Search

`/` opens live search across tasks.

### Active project

`Shift-P` treats the selected task as the active project context. Running it again on the same task clears the active project.

The active project is shown in the State pane. When an active project is set:

- quick create links new tasks to that project via the `projects` field
- the default Project view (`7`) shows tasks whose `projects` links resolve to the active project
- switching views does not clear the active project

## Configuration

### Views

Views are configured in `tasknotes-tui.yaml` under the `views` key. Built-in view kinds:

- `all`, `open`, `date`, `overdue`, `tracked`, `archived`
- `status` — filter by a status value
- `expression` — filter using mdbase expression syntax

Expression views have access to task fields (`status`, `priority`, `due`, `scheduled`, etc.) and special variables (`focusDate`, `today`, `isCompleted`, `isTracked`, `isArchived`, `path`).

Project-aware expression helpers are also available:

- `hasActiveProject`
- `activeProjectPath`
- `activeProjectTitle`
- `isActiveProject`
- `projectPaths` — resolved project targets for the current task

```yaml
views:
  6:
    label: "Doing Today"
    kind: "expression"
    expression: 'status == "doing" && (scheduled == focusDate || due == focusDate)'
  7:
    label: "Project"
    kind: "expression"
    where: "hasActiveProject && projectPaths.contains(activeProjectPath) && path != activeProjectPath"
```

### Keybinds

Keybinds map keys to commands. Multiple keys can map to the same command.

```yaml
keybinds:
  ctrl-p: command_palette
  n: create_task
  c: quick_create_task
  shift-p: set_active_project
  i: open_in_editor
  shift-t: toggle_time_tracking
  h: focus_prev_day
  l: focus_next_day
```

### Archive

Archive behavior is configured in `tasknotes.yaml`:

```yaml
archive:
  move_on_archive: false
  folder: "TaskNotes/Archive"
  tag: "archived"
  field: "archived"
```

## Time tracking

`T` starts or stops a timer on the selected task. Active timers show in the task list and detail pane. View `5` (Tracked) shows all tasks with a running timer.

## Recurring tasks

Tasks can have recurrence rules (RRULE syntax) with an anchor of either `scheduled` or `completion`. `S` skips the current instance without completing it.

## Development

```bash
cargo run --bin tasknotes-tui -- --root /path/to/vault
cargo run --bin tasknotes-tui -- --root /path/to/vault --focus-date 2026-03-29
```

### Demo vault

```bash
# Seed sample data
tasknotes-tui --root docs/demo-vault seed-demo-vault

# Render a snapshot to stdout
tasknotes-tui --root docs/demo-vault --focus-date 2026-03-29 \
  render-snapshot --width 120 --height 32
```

### Conformance testing

The repo includes adapters for running the TaskNotes spec test suite:

```bash
npm run conformance:test
npm run conformance:test:rust
npm run conformance:test:reference   # against the mdbase-tasknotes reference impl
```
