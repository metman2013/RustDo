# Changelog

What's new in each RustDo release. The app shows the newest entries the first time it
starts after an update, and each GitHub release uses its section as the release notes.

Format: `## <version> - <date>`, then `### New`, `### Improved` or `### Fixed`, then one
`- ` line per change, written for people who use RustDo (plain text, no Markdown).

## 0.2.6 - 2026-11-22

### Fixed
- Windows: automatic updates could fail and leave RustDo closing itself at every start without reopening. The background reminder helper kept the program file in use, so the installer couldn't replace it. The installer and RustDo now stop the helper first, and the helper starts again when the new version opens.
- Windows: if an automatic update ever doesn't finish, RustDo no longer retries it on its own at every start. It stays open and offers "Update & restart" instead.

## 0.2.5 - 2026-11-22

### New
- Sticky notes on your desktop (Windows and Linux). Each note is its own little window: drag it by its top bar, resize it from the corner, and use + to add another. Open "Sticky notes" in the sidebar to see them all, show or hide them, or delete them.
- Notes can be any colour: pick a ready-made one or choose any colour you like. Text size is adjustable, and the text colour follows the note colour automatically (or pick your own).
- Keep a note on top of other windows from its 🎨 menu.
- Sticky notes stay on this computer: they are saved next to your tasks and are never synced to a RustDo Hub or shared.
- Times on a 12-hour clock with AM/PM, now the default. Prefer 21:30? Choose the 24-hour clock in Settings under Time.
- This "What's new" message after each update. You can open it again from Settings.

### Improved
- Reminders that come due while RustDo is open now share one box in the corner instead of piling up on top of each other. Collapse it to a single line, use Dismiss all, and it opens again when a new reminder arrives.
- Phones: Settings opens as its own full-screen page that scrolls, with a Back button.
- Phones: dialogs (join or share a list, import, restore a backup, delete a list) fit the screen and sit near the top so the keyboard doesn't cover them.
- Phones: reminders and the undo message stay above the navigation bar.
- Desktop: the settings menu scrolls when the window is short instead of running off the bottom.
- The reminder editor has an AM/PM switch when using the 12-hour clock.

### Fixed
- The family sharing hint showed an empty box instead of an arrow.

## 0.2.1 - 2026-11-22

### Fixed
- Android: garbled text on some phones (such as the Pixel 10), where letters were cut off on one side and showed a sliver of the next letter.

## 0.2.0 - 2026-11-22

### New
- Family sharing with RustDo Hub: share a list with people at home through a small server on your own network (a Windows program or a Docker image). Shared lists sync every 30 seconds and shortly after a change.

### Improved
- Android: sharper text on screens with fractional scaling (Settings, Sharp text).

## 0.1.8 - 2026-11-19

### New
- Reminder notifications. Windows shows them even when RustDo is closed (a small helper starts with Windows); Linux and the web app show them while RustDo is open.

## 0.1.7 - 2026-11-19

### New
- A link to the RustDo Discord in Settings.
- The web app links to the desktop and mobile downloads.

## 0.1.6 - 2026-11-19

### New
- Linux: AppImage and .deb downloads.

## 0.1.5 - 2026-11-19

### New
- Windows: the installed app updates itself when it starts (on by default, and it waits if you're in the middle of typing).

## 0.1.4 - 2026-11-19

### New
- Touch mode: bigger buttons, text and spacing as soon as you use a touchscreen.

### Improved
- Windows updates install silently and reopen RustDo.

## 0.1.3 - 2026-11-19

### Fixed
- Android: the app now measures the status and navigation bars correctly on more phones.

## 0.1.2 - 2026-11-19

### Fixed
- Android: content stays clear of the status bar, navigation bar, notch and keyboard.

## 0.1.1 - 2026-11-19

### New
- Pull down to refresh.

## 0.1.0 - 2026-11-19

### New
- RustDo: a local-first to-do app with My Day, Important, Planned, lists, steps, due dates, reminders and repeating tasks.
- Import from Microsoft To Do.
- Daily automatic backups, with export and restore.
- Windows installer, web app and Android app.
