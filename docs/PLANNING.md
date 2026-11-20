# Selfkit — Planning Doc

> Note (2026-11-14): the tasks app "Tickd" was renamed **Agenda** (`com.selfkit.agenda`). This doc keeps the original name.

Nov 14, 2026

## Overview

Selfkit is a suite of five personal apps for one user on one Android phone, one Windows PC and one Linux PC (i3 on X11). Every app works offline, keeps data encrypted, and stays fast on low-end hardware.

| App | Purpose |
| --- | --- |
| Chrono | Tracks what I do on my phone, PCs and browsers: apps, sites, time, peaks, trends |
| Hisab | Money: income, spending, budgets, loans lent and borrowed |
| Tickd | Tasks with descriptions, photos, reminders and CalDAV sync |
| Reflect | Stats dashboard built from Chrono, Hisab and Dusk |
| Dusk | Sleep: estimated from device use, plus a manual log |

Goals:

- Runs well within 300–500 MB RAM per app on weak devices.
- Fully usable offline; the server only syncs.
- Private: everything is encrypted on the device before upload.
- Distraction-free: each app opens straight into its one job.
- No lock-in: any Postgres, any S3-compatible storage, open export formats.
- English only for now; default currency USD.

## Tech stack and platforms

All five apps are built with Tauri 2 from one codebase, picked over Slint and egui because the 300–500 MB budget makes room for a modern, animated UI.

| Layer | Choice | Notes |
| --- | --- | --- |
| App framework | Tauri 2 | Linux, Windows and Android from one codebase |
| Backend logic | Rust | Data, sync, encryption, file I/O |
| UI | Svelte + Tailwind + a component library | Small bundle for weak devices; all assets bundled, nothing loaded from a CDN |
| Local database | SQLite with full-text search | Source of truth on each device |
| Android extras | Small Kotlin plugins | Usage stats, background jobs, app-to-app data sharing |
| Background service | Rust Selfkit agent (desktop) | No UI, about 5–15 MB RAM |
| Browser tracking | WebExtension (Chrome + Firefox) | Talks to the agent through native messaging |

Offline rules:

- Bundle fonts (`@fontsource`), icons and every library; no CDN links.
- Windows installer uses WebView2 `offlineInstaller` mode, so it installs without internet.

Performance rules:

- Animate only `transform` and `opacity`; animations under 200 ms.
- Virtual scrolling for long lists.
- Release builds with LTO and stripped symbols.
- Test early on the weakest Linux machine (WebKitGTK is the slowest engine).

Testing:

- Linux: native.
- Windows: cross-compile, smoke-test under Wine, real tests on a GitHub Actions Windows runner.
- Android: emulator plus the real phone.
- A sample-data generator fills each app with months of fake data for testing charts.

Updates: Tauri's built-in updater with GitHub Releases from a private repo.

## Architecture

Five separate apps share one Rust core and one design system, so each app stays focused while the plumbing is written once.

&#91;embedded content: Selfkit architecture · PC, phone and replaceable server\]

Each device keeps working with the server gone; sync only moves encrypted data between devices.

- **Separate apps, one monorepo:** each app has its own icon and window and opens straight into its job. Shared: the `selfkit-core` Rust library (crypto, sync, storage, data models, bundles), the UI component library and the build pipeline.
- **Selfkit agent (Linux, Windows):** one small Rust background service with no UI. It does activity tracking, sync for all apps, backups and archives, reminders, shared settings and local diagnostics. Linux: systemd user service or an `exec` line in the i3 config, optional tray icon in i3bar. Windows: per-user startup task, not a Windows Service (a Service cannot see the active window).
- **Diagnostics stay local:** error and crash logs, sync status, storage used, last backup, the agent's own CPU and RAM. Nothing is sent anywhere unless I choose to sync it, encrypted, to my own server.
- **Android:** no always-on service. Sync and backups run as scheduled background jobs, reminders use the alarm system, and usage data comes from Android itself.
- **Apps talking to the agent:** Unix socket on Linux, named pipe on Windows; no network ports.
- **Same-device data exchange without a server:** on desktop, apps read each other's data through the shared Selfkit data folder. On Android, apps share through a ContentProvider limited to apps signed with my key. Reflect and Dusk therefore work offline.

Repo layout:

- `selfkit-core/`: shared Rust library
- `selfkit-agent/`: desktop background service
- `apps/chrono`, `apps/hisab`, `apps/tickd`, `apps/reflect`, `apps/dusk`: Tauri apps
- `ui/`: shared Svelte components and design tokens
- `extension/`: Chrome and Firefox extension
- `android-plugins/`: Kotlin plugins

## Data, storage and sync

Each device's SQLite database is the source of truth; the server is only a replaceable sync point, and every piece of data lives in at least two places.

**Where data lives**

| Data | Phone | PCs | Server |
| --- | --- | --- | --- |
| Records (all apps) | All of it, never pruned | All of it, never pruned | Postgres, encrypted rows |
| Photos and attachments | Last 90 days; older fetched on open, with a small cache | All photos | B2, encrypted files |
| Monthly archive bundles | — | Optional full copy | B2 |

- Keeping old photos locally can be switched off for chosen days, on both phone and PCs.
- Raw activity events and daily summaries are both kept forever.

**Server**

- Works with **any Postgres**, not only Neon. The app needs a connection string and nothing provider-specific: no Neon-only features, standard SQL, migrations that run on any Postgres version in common use.
- Starting provider: Neon free tier (0.5 GB, no card). Backup option: Aiven.
- Photos: any S3-compatible storage. Starting provider: Backblaze B2 (10 GB free).
- The health screen warns at 80% of the database limit.

**Sync**

- Changes queue locally and sync in batches every few minutes, or when an app opens. Every app works with no server at all.
- Records sync as encrypted blobs: id, collection, updated time, device id, ciphertext.
- Versioned schema migrations from day one.
- Timestamps stored in UTC plus the local time-zone offset.

**Conflicts**

- Two new items created on different devices: both are kept.
- The same item edited on two devices: a side-by-side preview with **Keep A**, **Keep B**, or **Merge field by field**; long text gets a line-by-line diff merge.

**Shared settings** (day start, main currency, theme): one Selfkit settings record. A change applies to all apps on the same device at once, and to the other devices through sync. Day start defaults to 00:00.

**Photos:** document-quality mode for receipts and proof: up to about 2400 px on the long edge, high-quality WebP, so small print stays readable.

## Security and encryption

Everything is encrypted on the device before it leaves, so the server and storage providers only ever hold unreadable data.

- **Encryption:** XChaCha20-Poly1305 for every record and photo.
- **Keys:** a random data key encrypts the data; a master passphrase, stretched with Argon2id, locks the data key. Changing the passphrase re-locks only that one key, so it is instant.
- **Recovery key:** created at first setup, shown once and printable; it unlocks the data on any of the three devices.
- **Secrets storage:** database URL, B2 keys and the unlocked data key live in the OS keychain (Windows Credential Manager, Linux Secret Service, Android Keystore), never in plain config files.
- **Setting up each app:** enter the passphrase once, or scan a setup QR code from a device already set up; after that, PIN or fingerprint.
- **App lock:** PIN or fingerprint on every app.
- **Android signing key:** back it up encrypted alongside the recovery key. Losing it blocks app updates and breaks offline app-to-app data sharing.
- **Privacy defaults:** Chrono records app names only, no window titles; incognito browser tabs are never tracked; an exclude list and a pause button are always available.
- **Plain CSV/JSON export** is possible but shows a warning that it is not encrypted.

## Backup, export, import and server migration

One portable file format, the Selfkit bundle (`.skb`), handles backups, exports, imports and moving to a new server.

**What a bundle holds**

- A manifest: app, date range, schema version, source device.
- Records as JSON lines.
- Photos and attachments.
- Everything encrypted with my key, so a bundle is safe to store anywhere.

**What it is used for**

- **Automatic backups:** a monthly bundle per app, uploaded to B2; a weekly full export is also made.
- **Manual export and import** by day, month, year or a custom range, per app.
- **Server migration:** add a new Postgres and storage connection, press "Upload everything", and the app pushes all local data and archive bundles.
- **Database integration:** bundles can be imported straight into any Postgres.
- **Plain export:** CSV/JSON for spreadsheets (unencrypted, with a warning).

**Restore test:** after setup, restore once from the recovery key and a backup on another device, to prove the backups work.

## UI principles

Every app is distraction-free: it opens to its main action and shows nothing else unless asked.

- **Open to the main action.** Hisab opens to "add expense" with the number pad ready; Tickd opens to Today plus a quick-add box.
- **No feeds, badges, streak pop-ups or suggestions.**
- **Notifications off by default;** I turn on only the ones I want (task reminders, loan due dates, bedtime).
- **Stats only on request,** on their own screen, never on the home screen.
- **Calm look:** muted colours, one accent colour, generous spacing, short animations with no bounce.
- **Themes:** see Layout and themes below.
- **Undo instead of confirmations:** deleting shows "Deleted · Undo" for a few seconds.
- **Desktop hotkeys:** keyboard shortcuts inside each app, plus a global quick-capture hotkey (an i3 keybinding on Linux) that opens a tiny "add expense" or "add task" window and closes after saving.
- **Android quick capture:** home-screen widgets to add an expense, a task or a sleep rating in two taps.

### Layout and themes

The reference design is the **split layout** (mocked up for Hisab): a summary card with monthly progress at the top, the last few entries below it, and a fixed input panel at the bottom with a big number pad, a sliding category row, note and photo, and one Save button.

**Themes kept:** A · Calm Light, B · Soft Dark, D · Paper & Mono. C · Tonal is dropped.

**Theme setting:**

- Mode: **System** (follows the device) or one fixed theme.
- In System mode, I choose which theme is used when the device is light (A or D) and which when it is dark (B, or any dark theme added later).
- The setting is shared by all Selfkit apps and syncs between devices.

**Design is a reference, not a rule.** New Selfkit apps start from this layout and these themes, but an app may use a different layout when it gives less distraction or better usability for that app's job. Any departure is written down with its reason in the design docs.

Mockups: [Selfkit UI style options](https://claude.ai/artifact/8aJtrVxbgP5iiF1YYm5jv4), pages "Split themes" (with the category manager) and earlier options.

## Apps

### Chrono — activity tracker

Shows what I do with my phone and PCs: how much, when and on what.

What it tracks:

- **Android:** per-app usage time, app opens and timeline (UsageStatsManager, "Usage access" permission); unlocks and pickups; notification counts per app.
- **Windows:** active app and idle time (Win32).
- **Linux (i3, X11):** active app from the window-manager hints and idle time from the X11 screensaver extension; no extensions needed.
- **Browsers:** active tab's domain, time spent and idle state, via the Chrome/Firefox extension; incognito excluded.

Features:

- A combined timeline across phone, PCs and browsers.
- Totals per app, per website, per device, and combined.
- Screen time, unlocks, longest session, first and last use of the day.
- An hour × weekday heatmap showing peak times.
- Daily, weekly, monthly, yearly and lifetime trends, with "this week vs last week" comparisons.
- Categories (study, work, social, entertainment) and a focus score.
- Idle detection, so time away is not counted.
- Optional goals and limits with alerts; a weekly report.
- Privacy: app names only, exclude list, pause button.

Next (first item after release, decided 2026-11-21 — P15):

- **Reconcile Android usage from daily totals.** Android drops an uninstalled app's raw usage events but keeps its daily aggregate (`queryUsageStats(INTERVAL_DAILY)`, `totalTimeVisible`). Each import also reads the aggregates for the last 2 days; when a package's aggregate is at least **5 min** more than what Chrono imported for that package and day, the difference is stored as one **untimed** `app_sessions` record for that day (app, device, minutes, no clock position).
- Untimed time counts in day, app, category and device totals, goals and the weekly report; it has no place in the timeline, hours, heatmap, first/last use or "longest session".
- No double counting: the record is keyed by (device, package, day) and replaced on every reconcile; it is recomputed as aggregate − imported sessions, so later imports of real events shrink it to zero (then deleted).
- Impact: Reflect's screen time includes it (totals only; late-night phone use ignores it); Dusk ignores it (no time of day, so no sleep evidence).

### Hisab — money

Tracks income, spending and loans, in USD by default with other currencies per transaction.

- **Accounts and income sources** as cards (cash, bank, mobile wallet), with transfers between them.
- **Transactions:** amount, currency, date, category, tags, notes and photos; one expense can be split across categories.
- **Categories** with icons, colours and a monthly budget shown as a progress bar.
- **Overview:** spent, remaining and balance for the period.
- **Trends:** daily spending through the month, by category, by year and lifetime; pie, line and calendar-heatmap charts; net worth over time.
- **Currencies:** totals convert to the main currency using rates fetched online, cached offline and editable by hand.
- **Loans (lent and borrowed):** person, direction, amount, date given, due date, title, description and proof photos; partial repayments logged one by one with the remaining amount; status bars for ongoing, due soon, overdue and finished; reminders before due dates.
- **Extras:** recurring transactions and subscriptions, savings goals, search and filters, auto-import from bank and mobile-wallet SMS on Android, CSV/PDF export.

**Category rules** (expense and income lists kept separately):

- Add as many categories as I want, from the add screen ("+ New") or the category manager.
- Ranking: "My order" (drag to reorder) or "Most used" (automatic). Top-ranked categories show first on the add screen.
- When they don't fit, the category row slides sideways; a fade at the edge shows there is more.
- Deleting a category archives it: it disappears from the add screen and the manager's main list, but every old record keeps it, so history and trends stay correct. Archived categories can be restored.

### Tickd — todo

A focused task app that opens to Today.

- Lists or projects, tasks, subtasks, priorities and tags.
- Descriptions in Markdown with attached photos.
- Due dates and times, reminders, repeating tasks.
- Quick add in plain language, e.g. "pay rent tomorrow 5pm #home".
- Views: Today, Upcoming, calendar and Kanban board.
- **Calendar:** two-way CalDAV sync (Nextcloud, Fastmail, iCloud, Radicale; Google needs extra OAuth setup), plus `.ics` export, import and a subscribable feed.
- Completed-task history, stats and a Pomodoro timer.

### Reflect — stats

For now, a read-only dashboard built from Chrono, Hisab and Dusk.

- A daily summary: screen time, top apps, money spent, tasks done, last night's sleep.
- Streaks, e.g. days under 3 hours of phone use or days within budget.
- Connections, e.g. late-night phone use against spending or sleep.
- Weekly and monthly life-review pages.
- The health screen for the agent and sync: tracking on, last sync, last backup, errors, storage used.
- Later: manual habits such as gym or reading.

### Dusk — sleep

Estimates and logs sleep with almost no effort.

- Automatic estimate from the last device use at night and the first in the morning (Chrono data, or Android usage data directly).
- A manual log to correct it: bedtime, wake time and a 1–5 quality rating.
- Duration trends, a consistency score and sleep debt.
- A bedtime goal and a wind-down reminder.
- Connections: phone use after midnight against sleep quality; sleep against the next day's tasks and spending.
- Later: smartwatch data through Android Health Connect.

## Documentation for AI handoff

The code will be built in a separate Claude Code chat, so the repo itself must carry every rule and decision: any AI (or a new chat after an interruption) reads the Markdown docs first and only opens code for the exact files it is changing.

**Rules for the coding AI** (written into `CLAUDE.md` at the repo root, with `AGENTS.md` pointing to it for other tools):

- Read `CLAUDE.md`, then `docs/STATUS.md`, then only the docs for the area being changed. Read code only when the docs don't answer the question.
- Every change updates its docs in the same commit: what it does, its files, its data, its status. Code without updated docs is not done.
- Keep `docs/STATUS.md` current while working: the task in progress, what's finished, the exact next step, and any open questions, so a new chat can continue mid-task.
- Record every decision and its reason in `docs/DECISIONS.md` (newest first, dated).
- Follow this planning doc's constraints: low-end hardware, fully offline, encrypted before upload, any Postgres, distraction-free UI, design as a reference not a rule.
- Never ask me to repeat these rules; they live in the repo.

**Doc map** (one file per topic, short, kept current):

| File | Holds |
| --- | --- |
| `CLAUDE.md` | AI rules above, read order, project one-liner, build/test commands |
| `docs/README.md` | Index of every doc |
| `docs/STATUS.md` | Current state, task in progress, next step, handoff log |
| `docs/DECISIONS.md` | Dated decision log with reasons |
| `docs/PROJECT.md` | Goals, constraints, the five apps (summary of this plan) |
| `docs/architecture.md` | Repo layout, components, data flow, how apps talk to the agent |
| `docs/data-and-sync.md` | Local storage, any-Postgres sync, conflicts, retention, bundles |
| `docs/security.md` | Encryption, keys, recovery key, keychains, signing key |
| `docs/design-system.md` | Layout pattern, themes A/B/D and their tokens, theme setting, components, accessibility, recorded departures |
| `docs/conventions.md` | Code style, naming, commits, how to write and update docs |
| `docs/components/` | One file each: `selfkit-core`, `selfkit-agent`, `browser-extension` |
| `docs/platforms/` | One file each: `linux`, `windows`, `android` (permissions, build, test, quirks) |
| `docs/apps/<app>/README.md` | Per app: purpose, screens, data model, status, **code map** (which file does what) |
| `docs/apps/<app>/<feature>.md` | Per sub-feature, e.g. Hisab `categories.md`, `loans.md`; Chrono `tracking.md` |
| `docs/apps/<app>/<platform>.md` | Platform-specific notes for that app, when they exist |
| `docs/templates/` | Templates for app, feature and handoff entries |

**Code maps:** every app and component doc lists its source files with one line each on what they do, so the AI jumps straight to the right file instead of searching the codebase.

**First task for the Claude Code chat:** create `CLAUDE.md`, `AGENTS.md` and the doc skeleton from this plan before writing any code.

## Build order and next steps

Build the shared core first, then start Chrono's tracker early so its history grows while the other apps are built.

1. **selfkit-core:** storage, encryption, keys, recovery key, sync, bundle format, migrations.
2. **Selfkit agent + Chrono tracker** (no UI yet), browser extension and Android usage reader; start collecting data.
3. **Tickd:** the simplest app; proves the whole stack end to end.
4. **Hisab.**
5. **Chrono dashboard.**
6. **Reflect and Dusk.**

Setup tasks:

- [ ] Create the Neon database and Backblaze B2 bucket
- [ ] Create the private GitHub monorepo and CI (Linux, Windows, Android builds)
- [ ] Generate and back up the Android signing key
- [ ] Set up a CalDAV server or account for Tickd
- [ ] After first setup, run a full restore test on another device
