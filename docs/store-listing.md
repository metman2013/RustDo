# Microsoft Store listing for RustDo

Text to paste into Partner Center, in the order its pages ask for it. Store limits are in
brackets. Package identity: `KoebleEffect.RustDo`, publisher **KoebleEffect**.

---

## Pricing and availability

- **Markets:** all markets.
- **Pricing:** Free. No in-app purchases.
- **Visibility:** Public audience, discoverable in the Store.
- **Release:** "As soon as possible after certification" (or a manual date for the first one).

## Properties

- **Category:** Productivity. Subcategory: none.
- **Privacy policy URL:** https://rustdo.app/privacy.html
- **Website:** https://rustdo.app
- **Support contact:** https://github.com/metman2013/RustDo/issues
- **"Does this product access, collect, or transmit personal information?"** No. Tasks and
  notes stay on the device. The only connections are to a RustDo Hub the user runs themselves,
  and links the user clicks.
- **Product declarations:** leave the defaults unless one clearly applies. RustDo works
  offline, doesn't need a Microsoft account, and doesn't depend on any other product.
- **System requirements:** Minimum: Windows 10 version 1809, x64. Keyboard and mouse or
  touch.

## Age ratings (IARC questionnaire)

Answer as a **productivity/utility app**. Suggested answers:

- Violence, fear, sexual content, language, drugs, gambling: **No** to all.
- Does it let users interact or exchange content? Lists can be shared only with people the
  user chooses, through a server they run themselves (RustDo Hub). There's no chat, no public
  content and no contact with strangers. If the questionnaire asks specifically about sharing
  with other users, answer **Yes, users can share content** to stay accurate. That usually
  adds only a "Users Interact" notice, not a higher age rating.
- Does it share the user's location? **No.**
- Digital purchases? **No.**

Expected result: suitable for all ages (for example PEGI 3 / ESRB Everyone), possibly with a
"Users Interact" notice.

---

## Store listing (English)

### Product name
RustDo

### Short title [50]
RustDo: tasks and sticky notes

### Short description [1,000; shown on the Store tile]
A fast, private to-do app with My Day, reminders, subtasks and sticky notes. No account, no
ads, no tracking: your tasks stay on your computer.

### Description [10,000]

RustDo is a fast, friendly to-do app that keeps your tasks on your own computer. No account
to create, no ads, no tracking, nothing to subscribe to. Just open it and start planning
your day.

**Plan your day**
Add tasks to My Day to focus on what matters today. My Day starts fresh every morning.
Star important tasks, give them due dates and reminders, and see everything coming up in
Planned, grouped into Today, Tomorrow, This week and Later.

**Break big jobs into small steps**
Add subtasks right after you create a task. Type "Bedroom: organize underbed storage", then
"put comics away", "vacuum under the bed", and tick them off one by one. Subtasks show right
under their task in the list.

**Reminders that actually remind you**
Get a Windows notification at the time you choose, even after you close RustDo. Snooze for
10 minutes, an hour or until tomorrow. Choose a 12-hour (AM/PM) or 24-hour clock.

**Sticky notes on your desktop**
Pin colourful sticky notes to your desktop: any colour you like, with adjustable text size
and colour. They stay on your desktop even after you close RustDo, and come back after a
restart. A small RustDo icon by the clock lets you add a note, show or hide them all, or open
RustDo.

**Lists for everything**
Make lists for home, work, groceries or a project. Set repeating tasks (daily, weekdays,
weekly, monthly or yearly), add notes, and find anything with search.

**Your data stays yours**
Your tasks and notes are saved on your computer, in your Documents folder, so they're easy
to find and easy to back up. RustDo makes a backup every day and can export and restore
backups whenever you like.

**Share lists with your family (optional)**
Run a small RustDo Hub on your home network and share lists such as groceries or chores with
the people you live with. Nothing goes through anyone else's servers.

**Coming from Microsoft To Do?**
Bring your lists, tasks, steps, due dates and reminders over with the built-in importer.

RustDo is free and open source. It's also available for Linux, Android and the web at
rustdo.app.

### What's new in this version [1,500]

Welcome to RustDo in the Microsoft Store! This version adds:
- Updates through the Microsoft Store, and a cleaner install with no security warnings.
- Sticky notes that stay on your desktop after you close RustDo, with a tray icon to add,
  show or hide them.
- Subtasks right from "Add a task", shown under their task in the list.
- A Help section in Settings to copy diagnostic info if anything goes wrong.

### Product features [up to 20, each 200]

1. My Day: focus on today's tasks, fresh every morning
2. Reminders with Windows notifications, even when RustDo is closed
3. Subtasks you can add right after creating a task
4. Sticky notes on your desktop, in any colour, that stay after you close RustDo
5. Due dates, repeating tasks and a Planned view of what's coming up
6. Lists for home, work, groceries and projects
7. Star important tasks and search everything
8. 12-hour (AM/PM) or 24-hour clock
9. Daily automatic backups, plus export and restore
10. Your tasks stay on your computer: no account, no ads, no tracking
11. Optional family sharing through your own RustDo Hub
12. Import from Microsoft To Do
13. Light and dark themes
14. Free and open source

### Search terms [up to 7, each 30 characters]

1. to do list
2. task manager
3. sticky notes
4. reminders
5. checklist
6. planner
7. productivity

### Copyright and trademark info [200]
© 2026 KoebleEffect. RustDo is free software under the MIT License.

### Additional license terms
RustDo is open-source software released under the MIT License:
https://github.com/metman2013/RustDo/blob/main/LICENSE

### Developed by
KoebleEffect

---

## Images to make

Ready-made screenshots (1920×1080, sample data) are in `docs/store-screenshots/`. To take new ones: shoot at 1920×1080 or larger, in light or dark mode (dark looks best with the
current colours). Hide any personal tasks first. The Store needs at least one screenshot;
four to six works best. Suggested shots, in this order:

1. **My Day** with several tasks: a due date, a reminder bell, a starred task, and one task
   expanded to show its subtasks.
2. **Task details** open on the right: subtasks, a reminder with AM/PM, a due date and repeat.
3. **Sticky notes** on the desktop in a few colours, with the RustDo tray menu open.
4. **Planned** view with tasks grouped by Today, Tomorrow and Later.
5. **Lists** in the sidebar (Groceries, Work, Home) with a list open.
6. **Settings** showing Background (sticky notes, Start with Windows) and the 12/24-hour clock.

Optional captions (one line each): "Focus on today", "Break it into subtasks", "Never miss a
reminder", "Sticky notes on your desktop", "See what's coming up", "Your data stays on your
PC".

The Store builds its logo from the package's own icons. A 300×300 Store logo, if
Partner Center asks for one, can be made from `assets/rustdo.png` (512×512).

## Submitting each new version

**Automatic.** When a release is published, the `store-submit` job in
`.github/workflows/build.yml` sends that version's `.msix` to the Store as a new submission
(using the Microsoft Store CLI, `msstore publish`). Certification usually takes a few hours to
a few days; Partner Center emails you. The listing, including "What's new in this version",
carries over from the previous submission, so update "What's new" in Partner Center if you
want it to describe the new version.

It needs, in the repository's Settings › Secrets and variables › Actions:

- Secrets `STORE_TENANT_ID`, `STORE_CLIENT_ID`, `STORE_CLIENT_SECRET`: a Partner Center
  Microsoft Entra application (Account settings › User management › Microsoft Entra
  applications) with the Manager role, and one of its keys. Keys expire: when the job fails
  to sign in, add a new key there and update `STORE_CLIENT_SECRET`.
- Secret `STORE_SELLER_ID`: Account settings › Legal info › Developer.
- Store ID: built into the workflow (9PK0DF0QBWWM); a repository variable `STORE_PRODUCT_ID`
  overrides it.

Without them the job does nothing. If it fails (for example the previous submission is still
in certification), the GitHub release is unaffected.

**By hand** (if the job failed or isn't set up):

1. Open that release's CI run on GitHub (Actions › Build › the run on `main`) and download
   the **store** artifact. It contains `RustDo-<version>.msix`.
2. In Partner Center, open RustDo, choose **Start update**, and upload the `.msix` under
   **Packages**. Add the version's changelog lines under "What's new in this version", then
   **Submit to the Store**.
