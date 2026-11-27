# Changelog

What's new in each RustDo release. The app shows the newest entries the first time it
starts after an update, and each GitHub release uses its section as the release notes.

Format: `## <version> - <date>`, then `### New`, `### Improved` or `### Fixed`, then one
`- ` line per change, written for people who use RustDo (plain text, no Markdown).

Translations live in `changelog/de.md`, `changelog/es.md` and `changelog/fr.md`, same
format with translated section names; the app shows "What's new" in the user's language.
Each release needs its entry in all three (a test checks the current version).

## 0.4.14 - 2026-11-28

### Fixed
- Android: in dialogs such as Turn on sync, the keyboard still opened and closed over and over. When the keyboard opened, the dialog was rebuilt to scroll, which deselected the field and closed the keyboard again. Dialogs now just move up above the keyboard, and the field stays selected. The same could happen with "+ New list" on a tablet in landscape.

## 0.4.13 - 2026-11-28

### Fixed
- Android app and web app on phones and tablets: typing in a field low on the screen ("+ New list", the sync and sharing boxes) made the keyboard open and close over and over, so the page bounced. The keyboard covered the field, which made it close, which uncovered the field, which opened it again. The keyboard now stays open while a field is selected, and the field scrolls up above the keyboard.
- Android: the page no longer follows readings of the keyboard's height that flicker between two answers while it slides in.

## 0.4.12 - 2026-11-28

### Fixed
- Android phones: since 0.4.11, windows such as What's new were squeezed against the screen edge and missing their Close button, because the navigation bar was taken for the keyboard. They look as before again, and still move above the keyboard when it's open.

## 0.4.11 - 2026-11-28

### Fixed
- Tablets and phones: the on-screen keyboard could cover the box you were typing in (for example when turning on sync). Dialogs now move into the space above the keyboard and scroll there, and the web app shrinks to fit above the keyboard instead of hiding behind it.
- Tablets in landscape (Android app and web app): the menu couldn't scroll down to New list, Import, Sticky notes and the bottom of the menu. On short screens the whole menu now scrolls.

## 0.4.10 - 2026-11-28

### Fixed
- Android: copying didn't reach the phone's clipboard, so a copied sync key, share code, list or backup couldn't be pasted into other apps. Every Copy button now uses Android's clipboard, and says "✔ Copied" for a moment.
- The hub address, sync key and share code fields have a 📋 Paste button, and Paste when you right-click (or long-press) them, on Windows, Linux, Android and the web, so a key copied elsewhere goes straight in.

## 0.4.9 - 2026-11-28

### Improved
- Phones: drag sticky notes into your own order by the ≡ handle on each note. The order syncs to your other phones (wider screens keep placing notes freely on the wall).
- While you drag a task or note near the top or bottom of the screen, the list scrolls along, so it can go anywhere in a long list.
- Windows and Linux: RustDo's tray icon now shows whenever RustDo runs in the background, including when it's only there for reminders. Its menu has "Quit RustDo completely", which also closes the window.
- Windows: Settings › Background has "Remind me even when RustDo is closed". Turn it off, together with background sticky notes, and closing RustDo closes it completely.
- The first time you close the window while RustDo keeps running in the tray, it tells you so, with a button to quit completely instead. Settings › Background also has "Quit RustDo completely".
- Microsoft Store version: while its window is closed, RustDo installs a waiting Store update by itself and starts again afterwards, instead of holding the update back by running in the tray.
- The ≡ handle for dragging tasks now shows on phones and tablets whatever the sort order; moving a task switches to "My order" by itself.
- After you add a task, its "Add a subtask" box opens right under it (not at the top of the list). Add another task and the box moves to that one; ✖ closes it.

### Fixed
- A list's page now shows the same icon as in the menu (🏠 for Tasks, 👥 for shared lists, 📋 for your own lists, 📚 for All), instead of ☰, which looked like the menu button.
- Phones and tablets: dragging a task down by its ≡ handle at the top of the list started "pull to refresh" instead of moving the task.

## 0.4.8 - 2026-11-27

### Fixed
- The Sticky notes page now starts like every other page: ☰ (on phones), the title, then "+ New note" and a More menu (back up, restore, tidy up), all on one line.
- Phones and tablets: tasks can now be dragged into your own order. In "My order", each task has a ≡ handle: drag it up or down (dragging elsewhere on a task still scrolls the list).
- Phones and tablets: lists can be dragged into a new order too, by the ≡ handle on each list in the menu.
- Sticky notes on phones and tablets have a bigger corner to resize them with a finger.

## 0.4.7 - 2026-11-27

### New
- Arrange tasks in your own order: drag a task up or down (or right-click or long-press it › Move up, Move down, Move to top). Sort switches to the new "My order", which syncs to your other devices and to shared lists.
- Sticky notes on phones and tablets have bigger + / 🎨 / 🗑 buttons that are easy to tap.
- RustDo for Linux on ARM64 (Raspberry Pi desktops, ARM laptops): .deb, .rpm, AppImage and .tar.gz.
- RustDo Hub for Linux without Docker: ready-made downloads for PCs (x86_64) and Raspberry Pi (arm64), and an install script that sets the hub up as a service that starts with the computer.
- RustDo for Linux now also comes as an .rpm (Fedora, openSUSE) and as a .tar.gz for any distribution, with an install script that adds RustDo to your app menu.
- Step-by-step RustDo Hub setup for Synology NAS (Container Manager), in the hub guide and in RustDo's own "How syncing and sharing work".

### Fixed
- Web app on Android phones and tablets: letters could look uneven, with some clipped or shifted (the graphics chip drew text with too little precision). Text is now drawn at full precision and looks as crisp as in the Android app.
- Android: Settings › Background was missing the "+ Picture…" button, so you couldn't use your own photos. It's there now, and opens RustDo's photo picker.
- Text is easier to read over background images: dates, hints and other light text no longer fade into the picture, in both the dark and light themes.

## 0.4.6 - 2026-11-27

### New
- Background images: choose one for the whole app in Settings › Background, and give any list its own (right-click or long-press the list › Background…). Pick from built-in backgrounds or your own picture (on Android, RustDo shows your photos once you allow it). "Dim background" keeps tasks easy to read.
- Backgrounds sync with Sync my devices, your own pictures too, so a picture chosen on your PC also shows on your phone. (Your RustDo Hub needs to be 0.4.6 or later to carry pictures.)
- Sticky notes can have a picture too: open a note's 🎨 menu › Picture and choose a built-in one or your own. The note's colour tints it so the text stays readable.
- Get RustDo (in the ⚙ menu) lists every way to get RustDo: the Microsoft Store, Windows installer, Android, Linux, the web app and the Hub.
- A "Join the RustDo Discord community" button at the bottom of the lists, for ideas, bugs and help.
- RustDo Hub can run as a Home Assistant app: in Home Assistant's app store, add the RustDo repository and install RustDo Hub. See the hub guide.

### Fixed
- Web app on phones and tablets: letters could look squeezed on some screens. "Sharp text" (on by default, in the ⚙ menu) now works there too, as in the Android app.
- RustDo Hub's Docker image now works when its data folder is owned by root (common on NAS boxes and in Home Assistant).

## 0.4.5 - 2026-11-26

### Fixed
- Android: after the on-screen keyboard was hidden (with its hide key, the back gesture or by switching apps), tapping a text field didn't bring it back. Tapping a field now always opens the keyboard, and it comes back when you return to RustDo.
- "Add a task" has an Add button as soon as you type, so a task can be saved without the keyboard's Enter key.

## 0.4.4 - 2026-11-26

### New
- Show list: a large, easy-to-read page with a list's tasks and their notes, for example a list of medications to show your doctor. Copy it as text to paste into an email or message, or on a computer print it or save it as a PDF. Find it next to the list's name, or right-click (long-press) a list.

### Improved
- Phones: a clear back button (an arrow in a rounded button) replaces the small "<" on the menu, Settings, task details and Sticky notes.
- Android: the back button and back gesture now work in RustDo. Back closes what's open, steps back to the list and to My Day, and from My Day leaves the app.
- Reorder your lists: drag a list up or down in the menu, or on a phone long-press it and choose Move up or Move down. The new order syncs to your other devices.
- New installs in German, Spanish or French start with a 24-hour clock, and the default list's name follows when you switch language.
- "What's new" now appears in your language (German, Spanish or French).
- RustDo Hub keeps a copy of its data every day (the last 14 days) in a backups folder next to hub.json.

## 0.4.3 - 2026-11-26

### New
- RustDo now speaks Spanish and French as well: Settings › Language offers Español and Français next to English and Deutsch, and Automatic picks them when your device uses them. Dates read the local way too, like "Miércoles, 25 de noviembre" and "Mercredi 25 novembre".

## 0.4.2 - 2026-11-25

### New
- Import straight from Microsoft To Do: choose Import from Microsoft To Do in the menu, then Sign in with Microsoft. RustDo reads all your lists, tasks, steps, notes, due dates and reminders and shows what it will import. It only reads your To Do data and never changes it. Works on Windows, Linux, Android and the web app, with personal and work or school Microsoft accounts.
- Android can now import from Microsoft To Do too (by signing in).

### Fixed
- German: a few longer messages that still showed in English are now translated.

## 0.4.1 - 2026-11-25

### New
- RustDo speaks German: Settings › Language (Sprache) offers Automatic, English and Deutsch. Automatic follows your device's language. Dates read the German way too, like "Mittwoch, 25. November".
- Language support is built in from here on, so more languages can follow.

## 0.4.0 - 2026-11-25

### New
- Sync my devices: keep your lists, tasks, sticky notes and settings (theme, clock, sort order) the same on all your devices, including the web app, through your own RustDo Hub. Turn it on in Settings under Sync my devices: your first device gets a sync key, and your other devices use the same hub address and key. Each device keeps its own reminder pop-ups and where its sticky notes sit on screen.
- Lists you share with family stay separate from your own sync: the people you share with see only the shared list, and your other devices join it by themselves.
- RustDo Hub 0.4.0 is needed for Sync my devices (update the Docker image or the Windows hub). It also lets the web app connect when the hub is on an https:// address.
- The hub guide in Settings now covers syncing your own devices as well as sharing lists.

## 0.3.3 - 2026-11-25

### New
- RustDo is in the Microsoft Store: search for RustDo or use the "Get it from Microsoft" button on rustdo.app. The Store version installs without a security warning and updates itself through the Store.

- Settings › Family sharing has a new "How family sharing works" guide: how to start a RustDo Hub (Windows or Docker, with a button to copy the Docker command), share a list and join it on other devices. The share dialog links to it too.

### Fixed
- Sticky notes: choosing a custom note or text colour now opens the colour picker inside the note's menu, with a Done button, and the menu opens on its usual options again next time. A small note grows while its menu is open so everything fits, then goes back to its size.

## 0.3.2 - 2026-11-24

### New
- Getting ready for the Microsoft Store: RustDo can now be installed as a Store app, which Windows trusts without the "Windows protected your PC" warning. The Store version updates through the Store, and its tray icon starts at sign-in the Store way. The regular installer and portable download stay as they are.
- A privacy policy at rustdo.app/privacy.html: RustDo has no accounts, analytics or tracking, and your tasks and notes stay on your devices.

## 0.3.1 - 2026-11-24

### New
- The bottom of the menu now shows the RustDo version, a "What's new" link and a link to the RustDo Discord (they're still in Settings too).
- Windows and Linux: Settings has a new Help section. "Copy diagnostic info" copies your RustDo version, system details and recent log lines to paste into a report; "Open logs folder" shows the log files.
- Windows and Linux: RustDo keeps a small log of each start and of any problems (in RustDo's logs folder), and if it ever has to close because of an error, it says so and where the details are, instead of disappearing.

### Fixed
- Windows: RustDo no longer needs the Microsoft Visual C++ Redistributable. Without it, RustDo 0.3.0 and earlier couldn't start on some PCs.
- Windows: if the graphics driver can't run RustDo with OpenGL (common in virtual machines, over Remote Desktop, or with a missing driver), RustDo now switches to Direct3D instead of not opening, and remembers that for next time.
- Windows: the installer now says that RustDo needs Windows 10 or later, instead of installing on Windows 7 or 8 where it can't run.
- Phones: the Sticky notes buttons fit on the screen (they ran off the left edge), and the text under them wraps instead of being cut off.
- Android: the Sticky notes page says notes are kept on this phone (not "in this browser") and asks you to tap, not click.

## 0.3.0 - 2026-11-24

### New
- Sticky notes on Android. Open "Sticky notes" in the menu: on a phone your notes are a column of colourful cards you can type in, recolour, resize the text of, and delete; on a tablet they're a wall you can arrange by dragging, like the web app.
- Android notes are saved on your phone and included in the daily automatic backups. "Copy backup to clipboard" includes them, "Back up notes" copies just your notes, and "Restore from pasted backup" accepts either.

## 0.2.9 - 2026-11-23

### New
- Subtasks right from "Add a task": after you add a task (say "Bedroom - Organize underbed storage"), a "Subtasks for" box appears underneath. Type "Put comics away", press Enter, and keep going.
- Subtasks show under their task in the list: click the small triangle next to the star to show or hide them, tick them off there, or add more. Right-click (long-press on a phone) a task and choose "Add a subtask" for a task that has none yet.
- Steps are now called subtasks everywhere; your existing steps are your subtasks.

### Fixed
- Windows and Linux: typing in a task's "Add a subtask" box no longer makes the details panel keep growing wider (and grow back after you resized it).

## 0.2.8 - 2026-11-22

### New
- Sticky notes in the web app: the Sticky notes page is now a wall of notes. Drag a note by its top bar to move it, drag its corner to resize it, and the note you touch comes to the front. Tidy up lines them all up neatly.
- Web notes can be any colour, with adjustable text size and colour, just like on the desktop. They're kept in your browser and included in Export backup; Back up notes and Restore notes download and open a notes file.
- On a phone-sized screen, the wall becomes a simple column of notes.

### Fixed
- Dragging a note on a touch screen no longer makes it jump on the first touch.

## 0.2.7 - 2026-11-22

### New
- Sticky notes stay on your desktop after you close RustDo. A RustDo icon in the system tray (by the clock on Windows) keeps them running: click it to open RustDo, or right-click it to add a note, show or hide all notes, or quit.
- The tray icon starts when you sign in, so your notes come back after a restart. Turn this off in Settings under Background ("Start with Windows"), or turn the background notes off completely ("Keep sticky notes running in the background").
- On Windows, reminders now come from the same tray companion, so they keep arriving after you close RustDo, as before.

### Improved
- Adding steps to a task is easier to spot: the step box now looks like a box, the blue + adds the step, and an Add button appears as you type.

## 0.2.6 - 2026-11-22

### New
- Sticky notes are now backed up. The daily automatic backup saves them next to your task backups (and to your extra backup folder, if you set one), keeping the last 14 days.
- Export backup includes your sticky notes, and Restore from backup brings them back. Older backups without notes still restore and leave your current notes alone.
- Back up or restore just your sticky notes, from Settings under Backups or from the Sticky notes page. Restoring notes keeps the ones you have: notes from the backup are added, and a note in both keeps whichever was edited last.

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
