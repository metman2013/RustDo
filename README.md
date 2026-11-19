# RustDo

A fast, local-first desktop to-do app written in Rust ([egui](https://github.com/emilk/egui)).
It's modeled on Microsoft To Do's workflow, with no account, cloud or telemetry.
Your tasks live in one JSON file on your machine.

## Features

| Microsoft To Do | RustDo |
|---|---|
| My Day (resets every day) | ✅ `☀ My Day`. Tasks added to it drop out automatically the next day. |
| Important | ✅ Star any task |
| Planned | ✅ Grouped into Earlier / Today / Tomorrow / Next 7 days / Later |
| All, Completed | ✅ |
| Custom lists | ✅ Create, rename (double-click or right-click), delete |
| Steps (subtasks) | ✅ Progress shows as "1 of 3" |
| Due dates | ✅ Today / Tomorrow / Next week presets, plus a date picker. Overdue tasks show in red. |
| Reminders | ✅ In-app popup with Complete / Open / Snooze (10 min, 1 h, tomorrow). The taskbar icon flashes. |
| Repeat | ✅ Daily, weekdays, weekly, monthly, yearly. Completing a task schedules the next one. |
| Notes | ✅ |
| Search | ✅ Searches titles, notes and steps |
| Sort | ✅ Newest, due date, A–Z, importance |
| Light / dark theme | ✅ System / light / dark (button next to the logo) |
| Undo delete | ✅ |
| Bring your tasks over | ✅ Import from Microsoft To Do (see below) |

### Keyboard shortcuts

| Key | Action |
|---|---|
| `Ctrl+N` | New task |
| `Ctrl+F` | Search |
| `Enter` | Save the task or step you're typing |
| `Del` | Delete the selected task (you can undo) |
| `Esc` | Clear the search, or close the details pane |

Right-click a task for quick actions: My Day, importance, complete, delete.

## Importing from Microsoft To Do

Microsoft To Do has no export button. RustDo reads your tasks through Microsoft's own
Graph API instead, which keeps lists, steps, notes, due dates, reminders, repeats,
importance and completed tasks.

**1. Export.** In PowerShell, from this folder, run:

```bash
powershell -ExecutionPolicy Bypass -File .\tools\Export-MicrosoftToDo.ps1
```

The script installs Microsoft's `Microsoft.Graph.Authentication` module for your user
account if needed. It then asks you to sign in to Microsoft and writes
`mstodo-export.json` to your Desktop. It only asks for the read-only `Tasks.Read`
permission and changes nothing in your account.

- Personal account (outlook.com, hotmail, live) and sign-in fails? Add `-TenantId consumers`.
- No sign-in window appears? Add `-UseDeviceCode` and sign in with the code at microsoft.com/devicelogin.

**2. Import.** In RustDo, click **Import from Microsoft To Do…** at the bottom of the
sidebar, or drag the file onto the window. A preview shows what will be added before
anything changes.

- Your built-in "Tasks" list goes into RustDo's Tasks list. Lists with the same name are merged.
- Completed tasks are left out unless you tick the box.
- Importing the same file again skips tasks you already imported. This is safe to repeat.
- Your current data is backed up to `tasks.before-import-<timestamp>.json` first.
- Reminders that are already in the past won't pop up.

**Other import sources:**

- **Graph Explorer.** At developer.microsoft.com/graph/graph-explorer, sign in and run
  `GET https://graph.microsoft.com/v1.0/me/todo/lists/{list-id}/tasks?$expand=checklistItems`.
  Save the response as `<List name>.json` and import it. The list is named after the file.
- **Classic Outlook CSV.** Use *File → Open & Export → Import/Export → Export to a file →
  CSV*, then pick a Tasks folder. RustDo needs the English column names. The list is named
  after the file, and `Tasks.csv` goes into the Tasks list.

What doesn't carry over: My Day (Microsoft doesn't expose it), file attachments, and
shared-list membership. Unusual repeats, like every 2 weeks or several days a week,
become the nearest simple repeat. The preview lists each one.

## Build & run

You need Rust (<https://rustup.rs>). On Windows, either toolchain works:

- **MSVC** (default): needs the Visual Studio C++ Build Tools.
- **GNU**: needs a full MinGW-w64 on `PATH`, for example `winget install BrechtSanders.WinLibs.POSIX.UCRT`.
  Rust's bundled `dlltool` can't run without `as.exe`.

```bash
cargo run --release
```

The binary is `target/release/rustdo.exe`, a single self-contained file. Copy it anywhere.

For binaries you publish, build with `tools\build-release.ps1`. By default Rust embeds
source paths that include your Windows user folder. The script rewrites them to neutral
names and then checks that none are left.

```bash
cargo test
```

## Your data

| | |
|---|---|
| Location | `%APPDATA%\RustDo\data\tasks.json` (Linux: `~/.local/share/rustdo/`, macOS: `~/Library/Application Support/RustDo/`) |
| Override | Set the `RUSTDO_DATA` environment variable to a file path |
| Backup | `tasks.json.bak` holds the last copy that loaded successfully |
| Safety | Saves are atomic (write to a temp file, then rename). If the file is damaged, it's renamed to `tasks.corrupt-<timestamp>.json` and never overwritten. |

To sync between PCs, point `RUSTDO_DATA` at a file in OneDrive, Dropbox or Syncthing.
Only run the app on one machine at a time.

## Known limitations

- Reminders fire only while RustDo is running. Minimizing is fine, closing isn't. There are no Windows toast notifications yet.
- There's no built-in cloud sync or sharing of lists with other people.
- The export script hasn't been tested against every kind of Microsoft account
  (work, school and personal accounts sign in differently). If it fails, use the Graph Explorer route.

## Project layout

```
src/main.rs     window setup
src/model.rs    data model + rules (My Day, recurrence, reminders) with unit tests
src/storage.rs  JSON load/save, backup, corruption handling
src/import.rs   Microsoft To Do (Graph JSON) and Outlook CSV import, with tests
src/app.rs      egui UI
tools/Export-MicrosoftToDo.ps1   read-only exporter using Microsoft Graph
```
