# tasknotes-tui

A fast terminal UI for TaskNotes-style markdown tasks, built in Rust on top of `mdbase-rs`.

## Architecture

- `src/repository.rs`: storage adapter on top of `mdbase-rs`
- `src/app.rs`: application state and user actions
- `src/ui.rs`: `ratatui` rendering and keyboard handling
- `src/spec_ops.rs`: TaskNotes-spec bridge surface
- `js/tasknotes-spec-adapter.mjs`: JS adapter for the TaskNotes-spec runner

The interactive TUI uses `mdbase-rs` for collection reads and writes. The repo also includes two conformance paths:

- `npm run conformance:test`: runs the mature reference TaskNotes-spec suite against `mdbase-tasknotes`
- `npm run conformance:test:bridge`: runs the same suite against the local Rust bridge surface in this repo

The second path is intentionally narrower today. It exists so the Rust core can be pushed toward spec parity incrementally without blocking the TUI itself.

The JS adapter defaults to fallback-first routing through `mdbase-tasknotes` for conformance runs. Set `TASKNOTES_TUI_BRIDGE_MODE=rust` to exercise the local Rust bridge first.

## Run

```bash
cargo run --bin tasknotes-tui -- --root /path/to/vault
```

The target vault must be readable by `mdbase-rs`, which means it should contain `mdbase.yaml` and a task type definition.

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
- `/`: search
- `x` or `space`: toggle completion
- `T`: start/stop time tracking on the selected task
- `S`: skip/unskip today's recurring instance
- `n`: create task
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

`Ctrl-P` opens a modal command panel with fuzzy filtering, keyboard navigation, and inline help. It exposes filters, editing actions, creation, refresh, completion toggling, and recurring skip actions from one place.

Task body editing is intentionally delegated to your external editor rather than being done inline in the TUI.

## Time Tracking

Time tracking is now supported as a task action and on the TaskNotes-spec bridge surface.

- `T`: toggle tracking for the selected task
- `5`: show tasks with an active timer

Tracked tasks are marked in the list and show `Tracking: active` in the details pane.

## Calendar

The right pane now includes a mini monthly calendar. The highlighted day is the current focused date, and dates with tasks are marked with `*`.

When the date filter is active with `2`, the task list shows tasks for the focused date rather than only literal today. Use `h`/`l`, left/right, `PgUp`/`PgDn`, or `g` to move around the calendar.

## Date Editing

Due and scheduled edits now open a date picker instead of defaulting to raw text entry.

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
