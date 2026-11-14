# Hisab (com.selfkit.hisab)

Status: **v0.1, usable on Linux and Android.** Money app: accounts, expenses/income/transfers, categories with budgets, splits, photos, currencies, loans with repayments and reminders, recurring items and subscriptions, savings goals, search and filters, Android SMS import (review queue), CSV/PDF export, desktop quick-capture popup, Android home-screen widget.

Design (approved 2026-11-15 from mockups): warm neutral greys (#f3f3f1 / #0e0e0d), one green accent (#2e7a4d light, #5dbb7e dark) matching the icon, red only for overdue / over budget / destructive. Soft cards, rounded number-pad keys, hand-made SVG charts, Inter with tabular numbers. Light/dark follow the system unless set in Settings.

Feature notes: [categories.md](categories.md) · [loans.md](loans.md) · [currencies.md](currencies.md) · [sms-import.md](sms-import.md)

## Screens
- **Home = add expense** (phone): top bar (History, Insights, Accounts, More), month summary card (spent, budget bar with an even-pace mark, remaining, balance), the 3 last-entered entries, then the fixed input panel: Expense / Income / Transfer, big amount with currency and account chips (Transfer adds a "to" chip), sliding category row with edge fade and "+ New", note (words with `#` become tags), date, photo, Split, number pad (hold ⌫ to clear) and Save. Saving shows "Saved · $12 · Food  Undo". The draft is saved on every key, so Back, a crash or a restart never loses what was typed.
- **History**: search (notes, tags, category/account names, dates, amounts) and filter chips (period, type, category, account, tag, amount range), grouped by day with day totals. Tap an entry → **edit sheet** (amount, currency, date, time, account(s), category or split parts, note, tags, photos, delete with undo).
- **Insights** (only on request): Month (spent / remaining / balance, cumulative daily line vs last month with budget line, category donut, budgets, income donut), Year (monthly bars, category donut, calendar heatmap), Lifetime (yearly bars, totals, net worth over time).
- **Accounts**: net worth (accounts + lent − borrowed) with trend, account cards (balance in own currency, ≈ main currency), Transfer (opens the add panel in Transfer mode), new/edit/archive account.
- **Loans**: Lent / Borrowed totals, cards with status pill (Overdue red, Due soon, Ongoing, Finished), remaining bar; detail sheet with repayments one by one (Add / All / remove with undo), proof photos, reminder line; new/edit form (person, direction, amount, currency, title, description, given, due, linked account, reminder days + time).
- **Savings goals**, **Recurring & subscriptions** (monthly / yearly cost), **Categories** manager, **SMS imports** review, **Settings** (theme, main currency, rates, category order, default account, loan reminders, SMS, desktop shortcut, export), **Export** sheet with the not-encrypted warning.
- **Desktop (≥ 900 px)**: sidebar with all sections and key hints; Home shows summary + stats + recent on the left and the input panel card on the right; sheets open as centred panels.
- **Quick-capture popup** (desktop): global shortcut (default Super+Shift+E, change in Settings, shared `shortcuts.add_expense`) or `hisab --capture`. Compact add panel; Enter saves and closes, Esc closes (draft kept).

Keyboard (desktop): digits / . / Backspace / Delete (clear) / Enter (save) on Home · N add · H history · I insights · A accounts · L loans · G goals · R recurring · C categories · / search · , settings · Esc closes the top sheet (asks first if a form has unsaved input).

Android Back (Tauri `onBackButtonPress`, not webview history: Chrome skips history entries pushed without a tap, which made the second Back leave the app): closes the topmost sheet (a form with unsaved input asks "Discard?"), then the page; on Home it moves the app to the background (`HisabNative.minimize`), with the draft kept.

## Data model (selfkit-core records in `hisab.db`)
Amounts are decimal numbers rounded to 2 places, in the record's `currency` (ISO code). Dates are local `YYYY-MM-DD`, times `HH:MM`; `created_at` is UTC ms. Every record also has core's `updated_at` (UTC ms) + `tz_offset`.
- `accounts`: `name, kind (cash|bank|wallet|card|other), currency, opening, archived, order`.
- `categories`: `name, kind (expense|income), icon, color, budget (monthly, main currency, 0 = none), archived, order`. Delete = `archived: true`.
- `transactions`: `kind (expense|income|transfer), amount, currency, date, time, account, category, splits [{category, amount}] (≥ 2 parts that add up to amount; category = first part), tags [], note, photos [photo ids], source (manual|sms|recurring|import), recurring (id), created_at`. Transfers: `to_account, to_amount` (in the destination account's currency; converted when not typed); category empty.
- `recurring`: `name, freq (daily|weekly|monthly|yearly), interval, start, next, end, active, subscription, template {kind, amount, currency, account, category, note, tags}`.
- `loans`: `direction (lent|borrowed), person, title, description, amount, currency, given, due, account (optional, moves its balance), photos [], repayments [{id, date, amount, note}], remind, remind_days, remind_time`.
- `goals`: `name, target, currency, deadline, contributions [{id, date, amount (negative = taken out), note}], archived`.
- `sms`: `key (dedupe), sender, body, at (UTC ms), status (pending|saved|dismissed), parsed {kind, amount, currency, merchant, hint}, tx`.
- `rates` (single record id `hisab-rates`): `base, rates {CUR: units per 1 base}, manual {CUR: …}, fetched_at`.
- Prefs (kv `hisab.prefs`, not synced): `theme, rank (manual|used), account, notify, sms, sms_since`.
- Add-screen draft (kv `hisab.draft`, not synced): the half-typed entry, written on every change and when the app goes to the background.
- Computed (never stored): account `balance, balance_main`; category `usage, spent`; loan `repaid, remaining, remaining_main, status, days_left`; goal `saved`; recurring `monthly`; tx `amount_main`.
- Reading from other apps (Reflect): open `hisab.db` read-only; spending per day = sum of `transactions` with `kind = expense` converted with `rates`.

## Photos
Picked in the webview (`<input type=file>`; on Android the system picker / camera), shrunk to ≤ 2400 px on the long edge and encoded as WebP quality 0.9 by canvas (document quality). Stored as files in `<data_dir>/hisab-photos/<uuid>.webp` (JPEG only if the webview can't encode WebP; WebKitGTK and Android WebView both can). Records keep the photo id. Not yet uploaded to S3: when core gets a local blob index, these files go through `sync::s3::BlobStore` (encrypted).

## Reminders
Loan reminders are off until turned on (Settings, or automatically the first time you save a loan with a reminder, which also asks Android for notification permission). Fire `remind_days` before `due` at `remind_time` local. Android: exact alarms via `tauri-plugin-notification` (manifest USE_EXACT_ALARM), rescheduled on every change and at startup (the plugin uses its default small icon; a monochrome `ic_stat_hisab` drawable is included for later). Desktop: 20 s timer while the app runs.

## Android extras
- `MainActivity.kt` exposes `window.HisabNative`: `setDark`, `smsPermission`, `requestSms`, `setSmsEnabled`, `readSms(sinceMs)`, `takeQueued`, `takeLaunchAction`, `minimize`.
- `SmsReceiver.kt`: queues SMS_RECEIVED messages in private prefs only while import is on.
- `AddWidget.kt` + `res/layout/widget_add.xml`: home-screen "+ Expense" widget; opens the app (action `com.selfkit.hisab.ADD`) on the number pad.
- Proguard keeps `@JavascriptInterface` methods. After `tauri android init`, re-apply: icons, `mipmap-anydpi-v26`, `values/ic_launcher_background.xml`, `adjustResize`, permissions, receivers, widget files, Kotlin files, proguard rules.

## Build and run
```bash
cd apps/hisab && ../../dev/heavy.sh bun 1500 bun install
bun tauri dev                                                      # Linux dev window (port 5175)
cargo build -j 6 -p hisab --features tauri/custom-protocol         # debug binary with embedded UI
cargo test -j 6 -p hisab                                           # model tests
SELFKIT_DATA_DIR=/tmp/hisab-demo cargo run -p hisab --example seed -- --fresh   # 7 months of sample data
```
Android APK: `dev/ci-apk.sh hisab 0.1.0` (GitHub Actions build, signed locally into `out/hisab-0.1.0-arm64.apk`). Test on a phone via `dev/phone-lease.sh acquire hisab` + `export ANDROID_SERIAL=…`. Screenshots without a window: `apps/hisab/scripts/shot.sh` (HISAB_DEMO params: `page=`, `sheet=loan|newloan|tx|split|category|export|goal|recurring|account`, `i=`, `amount=`, `cat=`, `kind=`, `save=1`; window size `HISAB_WINDOW_SIZE=WxH`).

## Code map
- `src-tauri/src/lib.rs`: Tauri setup, commands (`boot, snapshot, transactions, trends, save, get, remove, restore, generate_recurring, sms_*, rates_*, export_file, photo_save/get, prefs_set, settings_get/set, close_capture, reschedule, dev_hooks`), `reminders`, `shortcut` (global shortcut + capture window), env hooks.
- `src-tauri/src/model.rs`: collections, validation/normalisation, split parts, default categories. `stats.rs`: balances, net worth, category usage/spend, loan status, overview, trends, search/filter. `fx.rs`: rates + conversion. `recur.rs`: recurring generation. `sms.rs`: SMS parser + queue. `export.rs`: CSV + hand-written PDF. `photos.rs`: photo files.
- `src-tauri/examples/seed.rs`: sample data (`--fresh`, `--theme`).
- `src/App.svelte`: layout (phone pages / desktop sidebar), sheets, Back, keyboard, demo hooks. `src/Capture.svelte`: quick-capture popup.
- `src/lib/`: `api.ts` (types + commands), `state.svelte.ts` (snapshot, navigation, draft, save/undo helpers), `fmt.ts`, `icons.ts`, `photo.ts`, `rates.ts`, `sms.ts`, `native.ts`.
- `src/components/`: `Home`, `Summary`, `AddPanel`, `NumPad`, `CategoryRow`, `TxRow`, `History`, `Insights`, `Accounts`, `Loans`, `Goals`, `RecurringList`, `Categories`, `SmsReview`, `Settings`, `More`; sheets `TxSheet`, `SplitSheet`, `AccountSheet`, `CategorySheet`, `LoanSheet`, `GoalSheet`, `RecurringSheet`, `ExportSheet`, `CurrencySheet`, `PhotoViewer`; shared `SheetFrame`, `Seg`, `Confirm`, `Toast`, `Ic`, `CatBadge`, `Photo`.
- `src/charts/`: `Cumulative`, `Donut`, `Bars`, `Heatmap`, `Trend` (plain SVG, no library).
- `src-tauri/gen/android/app/src/main/java/com/selfkit/hisab/`: `MainActivity.kt`, `SmsReceiver.kt`, `AddWidget.kt`.

## Status and tests (2026-11-15)
- `cargo test -p hisab`: 19 tests (validation, splits, balances incl. linked loans, overview, trends/net worth, search filters, recurring incl. month-end clamping, SMS parser on bKash/Nagad/bank/card formats, CSV/PDF, photos, reminder time).
- Linux (Xvfb, real input via XTest): keyboard add (digits, category, note with #tag, Enter), transfer to a BDT account (auto-converted arrival amount), EUR expense, search with `/`, CSV + PDF export opened and checked (pdftotext), quick-capture popup via `--capture` and via the global shortcut Super+Shift+E, category drag-reorder and archive; every screen screenshotted light/dark, phone/desktop width.
- Android (user's phones, OPPO CPH2565 Android 15 and vivo V2111): number-pad add, BDT expense with online rates (open.er-api), receipt photo from the system picker (stored, shown in the entry), split edit, Back with unsaved changes → "Discard?", loan with repayment, **loan reminder fired** (notification "Test Sam owes you · 60.00 USD left") with the app in the background, savings goal + contribution, "+ New" category from the add screen (auto-picked, budget shown), drag-reorder, archive + restore, hand-edited rate, recurring item generated on save, CSV + PDF export to Download (pulled and checked), dark theme, Back through sheet → page → page → home → background, draft kept after Back + force-stop, widget launch intent (`com.selfkit.hisab.ADD`) opens the add screen.
- Not tested live: SMS import on a device (the emulator was switched off and the phones are personal, so their real inboxes were not read). The parser + review queue are covered by unit tests and the review screen by seeded SMS on desktop. The widget was not placed on a home screen (it would change the user's launcher); its provider is registered (`dumpsys appwidget`) and its intent tested.
- Known gaps: photos are not uploaded to S3 yet (no core blob index); notification small icon is the plugin default (Tauri's notification config rejects an icon key in this version); OPPO ColorOS refuses `pm clear` for test cleanup.
