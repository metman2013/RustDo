# Google Play

Notes for publishing RustDo on Google Play. The Android build (android/, Gradle) produces
both an APK (GitHub releases, rustdo.app) and an Android App Bundle (`RustDo-<version>.aab`,
in the release and in the CI artifacts) for Play.

## What the build already does for Play

- **App Bundle** (`.aab`), signed with the same key as the APK. In the Play Console, use
  *Play App Signing* and upload the `.aab`; keep the existing key as the upload key (or let
  Play generate one and register yours as the upload key).
- **Target API level 36** (Android 16), minimum Android 7 (API 24).
- **16 KB page size**: the Rust library is linked for 16 KB pages and CI checks the APK's
  alignment (Play requires it for apps targeting Android 15+).
- **Android's photo picker** for pictures: no photo or storage permission at all.
- **Version code** from Cargo.toml's version (`1<<24 | major<<16 | minor<<8 | patch`),
  the same formula as the cargo-apk builds before 0.5, so updates install over old APKs.

## Permissions, and what to say about them

| Permission | Why |
| --- | --- |
| `INTERNET` | Update check (GitHub), and syncing with the person's own RustDo Hub. |
| `POST_NOTIFICATIONS` | Reminder notifications. Asked once, when the first reminder is set. |
| `SCHEDULE_EXACT_ALARM` | Reminders on time. Optional: users can allow it in *Alarms & reminders*; without it reminders may be a few minutes late. Not `USE_EXACT_ALARM` (Play reserves that for alarm and calendar apps). |
| `RECEIVE_BOOT_COMPLETED` | Setting reminders again after the phone restarts. |

## Data safety form (draft)

- **Does your app collect or share any of the required user data types?** No.
  - Tasks, notes and pictures stay on the device. Syncing sends them only to a server the
    user runs themselves (their RustDo Hub), at an address they type in; the developer never
    receives them. Google's guidance treats data sent to a server the user controls, at
    their request, as not collected by the developer. Re-check this wording against the
    current Play Console help before submitting.
  - The update check is an ordinary request to GitHub with no user data.
- **Is all data encrypted in transit?** Answer for the case where data leaves the device:
  sync uses the address the user chooses; `https://` hubs are encrypted, plain `http://` on
  a home network is not. If the form needs a yes/no, "No" is the honest answer while plain
  `http://` hubs are allowed.
- **Can users request that data be deleted?** Yes: it's on their device (clear the app's
  data), and on their own hub.
- **Privacy policy URL:** https://rustdo.app/privacy.html (updated for 0.5: photo picker,
  imports, widget and reminders, Home Assistant).

## Store listing

- Category: Productivity. Contains ads: no. In-app purchases: no.
- Content rating questionnaire: a productivity tool with no user-generated content shared
  publicly, no chat (family sharing is between people who share a code privately).
- Screenshots: phone and 7"/10" tablet. docs/store-screenshots has the Microsoft Store ones
  for reference.

## Closed testing (new personal developer accounts)

Google requires at least **12 testers opted in for 14 days in a row** before you can apply
for production access (check the current numbers in the Play Console; they've changed
before).

1. Create a Google Group (e.g. rustdo-testers@googlegroups.com) and share the join link on
   the RustDo Discord. About 6 people so far.
2. Play Console › Testing › Closed testing: create a track, add the Google Group as the
   testers list, upload the `.aab`, roll out.
3. Share the opt-in link from the track's page with the group. Testers must opt in and
   install from Play (an APK from GitHub doesn't count).
4. Keep at least 12 opted in for 14 days; ship updates to the track as usual meanwhile.
5. Apply for production access from the Dashboard, answering the questions about testing.
