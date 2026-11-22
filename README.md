# RustDo

A fast, local-first to-do app written in Rust ([egui](https://github.com/emilk/egui)).
It's modeled on Microsoft To Do's workflow, with no account, cloud or telemetry.
Your tasks stay on your device.

One codebase runs on three platforms:

| Platform | How to get it | Where your tasks live |
|---|---|---|
| **Windows** | `RustDo-<version>-setup.exe` from [Releases](https://github.com/metman2013/RustDo/releases). It installs for your user only and doesn't need admin rights. A portable `.exe` is also attached. | `Documents\RustDo\tasks.json` |
| **Linux** | From [Releases](https://github.com/metman2013/RustDo/releases): `RustDo-<version>-x86_64.AppImage` works on any distro (`chmod +x` it, then run it), or install `rustdo_<version>_amd64.deb` on Ubuntu/Debian/Mint. Needs GTK 3, which desktop installs have. | `~/Documents/RustDo/tasks.json` |
| **Android** | `RustDo-<version>.apk` from [Releases](https://github.com/metman2013/RustDo/releases). Allow "Install unknown apps" for your browser when Android asks. | The app's private storage |
| **Web (PWA)** | Open **https://rustdo.app**, then choose **Install app** (desktop Chrome/Edge) or **Add to Home screen** (phones). It works offline. | The browser's local storage for that site |

Each device keeps its own task list. There's no sync between devices yet.

**Community:** join the RustDo Discord at **https://discord.gg/HtEFHsP4TK** for questions, ideas
and bug reports. In the app, it's under **⚙ → Join the RustDo Discord**.

## Notifications

Give a task a reminder (🔔 **Remind me** in its details) and RustDo notifies you at that
time. Turn them off under **⚙ → Notify me about reminders**.

| Platform | When you're notified |
|---|---|
| **Windows** | **Always, even after you close RustDo.** A tiny background helper (`rustdo.exe --reminders`: no window, very little memory) shows a Windows notification at the reminder time. It starts with RustDo, and the installed version also starts it when you sign in to Windows. Turning notifications off stops it; uninstalling removes it. |
| **Linux** | While RustDo is running (minimized is fine). |
| **Web** | While the app is open, after you click **Allow notifications** in ⚙. Browsers can't notify for a closed web page. |
| **Android** | In-app popup while the app is open. Real Android notifications are planned. |

## Backups

Everything is under **⚙ → Backups**.

| | Windows | Android | Web |
|---|---|---|---|
| Automatic daily backup | `Documents\RustDo\Backups\RustDo-backup-<date>.json`, last 14 days | In the app's storage, last 14 days (removed if you uninstall) | In the browser, last 7 days |
| Extra backup folder | **Also back up to a folder…** (e.g. OneDrive or a USB stick) | — | — |
| Manual export | **Export backup…** saves a `.json` wherever you choose, and the next export starts in the same folder | **Copy backup to clipboard**, then paste it into a note or email | **Export backup…** downloads a `.json` file. After 30 days without one, a banner offers to export |
| Restore | **Restore from backup…**, or drop the file on the window | **Restore from pasted backup…** | **Restore from backup…** |

The automatic backup is written when the app starts, then at most every 5 minutes
while you make changes. Restoring replaces all tasks and lists, after saving your
current ones to `Backups\RustDo-before-restore-<time>.json`. A backup file is the same
JSON as `tasks.json`, so it also works as a way to move your tasks to another device:
export on one, restore on the other.

## Sticky notes (Windows and Linux)

Put notes on your desktop, like Microsoft's Sticky Notes. Open **Desktop notes → Sticky
notes** in the sidebar and click **+ New note**. Each note is its own small window:

- Drag the top bar to move it and the bottom-right corner to resize it. **+** adds another note.
- The **🎨** menu sets the note colour (ready-made colours or any colour), the text size, and
  the text colour. Text colour is automatic by default: dark on light notes, light on dark ones.
- **Keep on top** keeps a note above other windows. **✖** hides a note; show it again from the
  Sticky notes page.

Notes show while RustDo is running (minimized is fine), and reopen where you left them.
They are saved in `notes.json` next to `tasks.json`. They're never synced to a RustDo Hub
or shared, and they aren't part of the task backups.

## Family sharing (RustDo Hub)

Share a list (groceries, chores, packing) with people at home, **without any cloud**.
You run a tiny **RustDo Hub** on one machine on your network: a Windows exe, or a Docker
container on a NAS, home server or Raspberry Pi.

```bash
docker run -d --name rustdo-hub --init --restart unless-stopped -p 7979:7979 -v rustdo-hub-data:/data ghcr.io/metman2013/rustdo-hub:latest
```

Then in RustDo: right-click a list (long-press on a phone) → **👥 Share list…** → enter
the hub's address → you get a code like `K7QM-2XPD`. Others choose **⚙ → Family sharing →
Join a shared list…** and enter the code. Shared lists sync every 30 seconds, and My Day
stays personal. Works in the Windows, Linux and Android apps.

Full guide, including the Windows exe and using it away from home: [docs/hub.md](docs/hub.md).

## Updates

- **Windows and Android** check this repository's latest GitHub Release at most once a day.
  When there's a newer version, a banner offers **Update now**:
  - **Windows**: **Update & restart** downloads the new installer and runs it silently, with no
    wizard and no clicks. RustDo closes, updates and reopens on its own in a few seconds.
    Your tasks are kept. The setup wizard is only used for the first install.
  - **Windows auto-update** (installed version, on by default): each time RustDo opens it
    checks for a new version and, if there is one, updates itself the same silent way. It
    waits if you've started typing a new task, and only acts right after launch; otherwise you
    get the banner. Turn it off under **⚙ → Install updates automatically**. The portable exe
    never updates itself.
  - **Android** opens the new APK download. Install it over the old version.
- To turn the check off, or check right away, use the **⚙** menu. The check is an anonymous
  request to GitHub's public API. No task data is sent. The only other network traffic is
  to your own [RustDo Hub](#family-sharing-rustdo-hub), and only if you share a list.
- **Web** updates itself. Each published build gets a new service-worker cache, and open
  copies reload into the new version.

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
| Reminders | ✅ System notifications (see [Notifications](#notifications)), plus an in-app popup with Complete / Open / Snooze (10 min, 1 h, tomorrow). |
| Repeat | ✅ Daily, weekdays, weekly, monthly, yearly. Completing a task schedules the next one. |
| Notes | ✅ |
| Search | ✅ Searches titles, notes and steps |
| Sort | ✅ Newest, due date, A–Z, importance |
| Light / dark theme | ✅ System / light / dark (button next to the logo) |
| Undo delete | ✅ |
| Bring your tasks over | ✅ Import from Microsoft To Do (see below) |
| Sticky Notes | ✅ On Windows and Linux (see [Sticky notes](#sticky-notes-windows-and-linux)) |

### Keyboard shortcuts

| Key | Action |
|---|---|
| `Ctrl+N` | New task |
| `Ctrl+F` | Search |
| `Enter` | Save the task or step you're typing |
| `Del` | Delete the selected task (you can undo) |
| `Esc` | Clear the search, or close the details pane |

Right-click a task (long-press on a phone) for quick actions: My Day, importance,
complete, delete.

On a touch screen, pull the task list down and let go to refresh. In the web app this
reloads, which also picks up a newly published version. On Android and desktop it re-reads
your tasks and checks for updates.

Narrow windows and phones get a one-column layout: tap ☰ for your lists, tap a task for
its details, and use **< Back** to return.

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

**2. Import.** In RustDo (Windows or web), click **Import from Microsoft To Do…** at the
bottom of the sidebar, or drag the file onto the window. A preview shows what will be added
before anything changes. The Android app doesn't import files yet; use the web app on your
phone if you need your tasks there.

- Your built-in "Tasks" list goes into RustDo's Tasks list. Lists with the same name are merged.
- Completed tasks are left out unless you tick the box.
- Importing the same file again skips tasks you already imported. This is safe to repeat.
- Your current data is backed up to `Backups\RustDo-before-import-<time>.json` first.
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

## Releasing a new version

GitHub Actions ([.github/workflows/build.yml](.github/workflows/build.yml)) builds everything.
Each push to `main` runs the tests and all builds, and publishes the web app.

To release:

1. Set the new version for every platform: `.\tools\set-version.ps1 0.2.0`.
2. Add a `## 0.2.0 - <date>` section to [CHANGELOG.md](CHANGELOG.md), written for people who
   use RustDo. The app shows it in **What's new** after the update, and the GitHub Release
   uses it as its notes. A test fails if the current version has no entry.
3. Get it onto `main`: merge a pull request, or commit and push.

That's it. When `main` has a version that hasn't been released yet, the workflow creates the
`v0.2.0` tag and a GitHub Release, and attaches:
- the Windows installer and portable exe
- the Linux packages
- the APK
- the RustDo Hub exe

It also publishes the Hub's Docker image as `:0.2.0` and `:latest`. Nothing is released
unless every build succeeds. Installed copies pick the release up through their update
check. Pushing a `v0.2.0` tag yourself also works.

### One-time setup

- **Web hosting:** the app is served from GitHub Pages at **https://rustdo.app**. The
  domain's DNS is at Cloudflare. See [docs/hosting.md](docs/hosting.md) for the exact
  settings.
- **Android signing:** Android only installs an update if it's signed with the same key as
  the installed app. Create a key once, and keep the file and password somewhere safe. If
  you lose them, users must uninstall before the next update.

  ```bash
  keytool -genkeypair -keystore rustdo-release.jks -alias rustdo -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=metman2013"
  ```

  `keytool` comes with any Java JDK, for example `winget install Microsoft.OpenJDK.21`.
  Then add two repository secrets under *Settings → Secrets and variables → Actions*:
  `ANDROID_KEYSTORE_BASE64` with the file's base64 (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("rustdo-release.jks"))`)
  and `ANDROID_KEYSTORE_PASSWORD`. Until those secrets exist, CI signs with a throwaway key.
  That APK installs fine, but later builds can't update it.
- **Windows SmartScreen:** the installer isn't code-signed, so Windows shows *"Windows
  protected your PC"* the first time. Click **More info → Run anyway**. Removing the warning
  requires a code-signing certificate, a paid service.

## Build locally

You need Rust (<https://rustup.rs>). On Windows, either toolchain works:

- **MSVC** (default): needs the Visual Studio C++ Build Tools.
- **GNU**: needs a full MinGW-w64 on `PATH`, for example `winget install BrechtSanders.WinLibs.POSIX.UCRT`.
  Rust's bundled `dlltool` can't run without `as.exe`.

| Target | Command |
|---|---|
| Desktop app | `cargo run --release` |
| Tests | `cargo test` |
| Release exe with no personal paths embedded | `tools\build-release.ps1` |
| Windows installer | `iscc /DAppVersion=0.1.0 installer\rustdo.iss`, with [Inno Setup 6](https://jrsoftware.org/isinfo.php) |
| Web app into `dist/` | `tools\build-web.ps1`, which needs `rustup target add wasm32-unknown-unknown` and `cargo install wasm-bindgen-cli` at the version in Cargo.lock |
| Serve the web app locally | `tools\serve-web.ps1`, then open http://localhost:8765 |
| Android APK | `cargo apk build -p rustdo-android --release`, which needs the Android SDK and NDK plus `cargo install cargo-apk` (CI does this for you) |
| Regenerate icons | `tools\make-icons.ps1` |

By default Rust embeds source paths that include your Windows user folder in binaries.
`build-release.ps1` and `build-web.ps1` rewrite those paths to neutral names.

## Your data

| | |
|---|---|
| Location | `Documents\RustDo\tasks.json` on Windows (`~/Documents/RustDo/` on macOS and Linux). If Documents is synced to OneDrive, so is RustDo. Uninstalling doesn't delete it. Version 0.1 used `%APPDATA%\RustDo\data`; that data is moved automatically, and the old file is kept as `tasks.json.moved-to-Documents`. |
| Override | Set the `RUSTDO_DATA` environment variable to a file path |
| Backup | `tasks.json.bak` holds the last copy that loaded successfully. See [Backups](#backups) for the daily backups. |
| Safety | Saves are atomic (write to a temp file, then rename). If the file is damaged, it's renamed to `tasks.corrupt-<timestamp>.json` and never overwritten. |

To sync between PCs, point `RUSTDO_DATA` at a file in OneDrive, Dropbox or Syncthing.
Only run the app on one machine at a time.

## Known limitations

- Android reminders only show while the app is open. Real Android notifications need a
  build-system change and are planned next. Linux and web notifications also need the app
  running; see [Notifications](#notifications).
- Only shared lists sync between devices, through a [RustDo Hub](docs/hub.md); the web
  app can't use a hub.
- **Android is the least tested platform.** It's built by CI but hasn't been tried on real
  phones yet. Typing relies on the on-screen keyboard sending key events, and some keyboard
  apps (e.g. predictive or swipe input) may not work fully.
- The export script hasn't been tested against every kind of Microsoft account
  (work, school and personal accounts sign in differently). If it fails, use the Graph Explorer route.

## Project layout

```
src/main.rs        desktop entry point
src/lib.rs         web (wasm) and Android entry points
src/app.rs         egui UI, with wide and phone layouts
src/model.rs       data model + rules (My Day, recurrence, reminders), with tests
src/storage.rs     saving: JSON file (desktop/Android) or browser storage (web)
src/import.rs      Microsoft To Do (Graph JSON) and Outlook CSV import, with tests
src/update.rs      GitHub Releases update check
src/platform.rs    Android hooks (data folder, on-screen keyboard)
src/sync.rs        family sharing: syncing shared lists with a RustDo Hub, with tests
hub/               RustDo Hub server (Windows exe / Docker image), its own workspace
android/           APK wrapper crate, launcher icons
web/               PWA page, manifest, service worker, icons
installer/         Inno Setup script for the Windows installer
tools/             build, export and icon scripts
```
