# tasknotes-tui

A fast terminal UI for TaskNotes-style markdown tasks, built in Rust on top of `mdbase-rs`.

## Architecture

- `src/repository.rs`: storage adapter on top of `mdbase-rs`
- `src/app.rs`: application state and user actions
- `src/ui.rs`: `ratatui` rendering and keyboard handling
- `src/spec_ops.rs`: TaskNotes-spec bridge surface
- `js/tasknotes-spec-adapter.mjs`: JS adapter for the TaskNotes-spec runner

The interactive TUI uses `mdbase-rs` for collection reads and writes. The repo also includes three conformance paths:

- `npm run conformance:test`: runs the TaskNotes-spec suite against this repo's adapter
- `npm run conformance:test:rust`: runs the same suite through the local Rust bridge
- `npm run conformance:test:reference`: runs the suite against the sibling `mdbase-tasknotes` reference implementation

`mdbase-tasknotes` is not part of the TUI runtime. It is only used in the conformance harness as a sibling reference implementation.

## Run

```bash
cargo run --bin tasknotes-tui -- --root /path/to/vault
cargo run --bin tasknotes-tui -- print-default-config
```

The target vault must be readable by `mdbase-rs`, which means it should contain `mdbase.yaml` and a task type definition.

The TUI reads an optional vault-specific `tasknotes-tui.yaml` from that same root. Keeping it in the vault root is the right default for this project: views and keybinds are part of how you work with a specific vault, not global machine state.

## Install

For local development:

```bash
cargo run --bin tasknotes-tui -- --root /path/to/vault
```

For direct install from git:

```bash
cargo install --git <repo-url> --bin tasknotes-tui
```

GitHub Releases can also ship prebuilt archives for:

- Linux x86_64
- macOS x86_64
- macOS Apple Silicon
- Windows x86_64

A release workflow is included at [`.github/workflows/release.yml`](/home/calluma/projects/tasknotes-tui/.github/workflows/release.yml). Tagging a release like `v0.1.0` will build platform archives and attach them to the GitHub Release.

The runtime reads the vault-root `tasknotes.yaml` TaskNotes-spec config for field mapping, defaults, archive behavior, and related task behavior. Task membership comes from the `mdbase` type definition.

## Keys

- `Ctrl-P`: open fuzzy command palette
- `h` / `l` or left/right: move focused date by one day
- `PgUp` / `PgDn`: move focused date by one week
- `g`: jump focused date to today
- `j` / `k`: move
- `1`: open tasks
- `2`: date filter for the focused day
- `3`: overdue
- `4`: all
- `5`: tracked
- `6`: archived
- `/`: search
- `x` or `space`: toggle completion
- `z`: archive or restore the selected task
- `T`: start/stop time tracking on the selected task
- `S`: skip/unskip today's recurring instance
- `n`: create task
- `c`: quick create for the focused date
- `e`: edit title
- `i`: open selected task in `$EDITOR`
- `d`: edit due date
- `s`: edit scheduled date
- `p`: edit priority
- `t`: edit status
- `R`: edit recurrence rule
- `A`: edit recurrence anchor
- `r`: refresh
- `q`: quit

## Command Palette

`Ctrl-P` opens a modal command panel with fuzzy filtering, keyboard navigation, and inline help. It exposes configured view slots from `tasknotes-tui.yaml` alongside editing actions, creation, refresh, completion toggling, archive toggling, and recurring skip actions.

Archiving marks tasks with the configured archive tag and field, and can also move files into the configured archive folder.

Archive semantics are configurable in `tasknotes.yaml`:

```yaml
archive:
  move_on_archive: false
  folder: "TaskNotes/Archive"
  tag: "archived"
  field: "archived"
```

`tag` and `field` control what the TUI writes and what it considers archived in views and filtering.

Task membership is defined by the `mdbase` task type. Files outside that type are not shown as tasks.

Task body editing uses your external editor.

## Time Tracking

- `T`: toggle tracking for the selected task
- `5`: show tasks with an active timer

Tracked tasks are marked in the list and show `Tracking: active` in the details pane.

## TUI Config

`tasknotes-tui.yaml` lets you configure keybinds and up to 9 privileged view slots for the number keys.

To print the exact default config:

```bash
tasknotes-tui print-default-config
```

Example:

```yaml
keybinds:
  ctrl-p: command_palette
  n: create_task
  c: quick_create_task
  i: open_in_editor
  shift-t: toggle_time_tracking
  h: focus_prev_day
  left: focus_prev_day
  l: focus_next_day
  right: focus_next_day

views:
  1:
    label: "Inbox"
    kind: "open"
  2:
    label: "Today"
    kind: "date"
  3:
    label: "Doing"
    kind: "status"
    value: "doing"
  4:
    label: "Overdue"
    kind: "overdue"
  5:
    label: "Tracked"
    kind: "tracked"
  6:
    label: "Archived"
    kind: "archived"
  7:
    label: "All"
    kind: "all"
```

The keybind table is `key -> command`. Multiple keys can point at the same command.

Supported view kinds:

- `all`
- `open`
- `date`
- `overdue`
- `tracked`
- `archived`
- `status` with `value`
- `expression` with `expression`, `where`, or `value`

Number keys `1` through `9` always activate the corresponding configured slot when present.

Expression views use the `mdbase` expression syntax and are evaluated against the in-memory task cache, so view switching stays fast after a reload. Example:

```yaml
views:
  6:
    label: "Doing Today"
    kind: "expression"
    expression: "status == \"doing\" && (scheduled == focusDate || due == focusDate)"
  7:
    label: "Tracked Work"
    kind: "expression"
    where: "isTracked && !isCompleted"
```

Available expression context includes normal normalized task fields like `status`, `priority`, `due`, `scheduled`, `timeEntries`, plus:

- `focusDate`: the currently focused calendar date
- `today`: today’s local date
- `isCompleted`: boolean derived from TaskNotes completed-status rules
- `isTracked`: boolean for an active time entry
- `isArchived`: boolean for archive state
- `path`: the task path
- `file.*`: `mdbase` file helpers such as `file.path`, `file.body`, tags, links, and related expression helpers

## Calendar

The right pane includes a mini monthly calendar. The highlighted day is the focused date, and dates with tasks are marked with `*`.

When the date view is active with `2`, the task list shows tasks for the focused date. Use `h`/`l`, left/right, `PgUp`/`PgDn`, or `g` to move around the calendar.

## Date Editing

Due and scheduled dates are edited through a date picker.

- arrows or `h`/`l`: move by day
- `j`/`k`: move by week
- `H`/`L`: move by month
- `t`: jump to today
- `c`: clear the value
- `/`: switch to manual `YYYY-MM-DD` entry
- `Enter`: save

## Create Flow

`n` opens a multi-step draft flow:

- title
- details
- due date
- scheduled date
- priority
- status
- recurrence rule
- recurrence anchor

Blank values skip optional fields.

## Quick Create

`c` opens a title-only quick create prompt.

- title comes from the prompt
- `scheduled` is set to the currently focused date
- status and priority use vault defaults
- details/body stays empty

Quick create is the fast capture path for calendar-driven planning.
