# Architecture

Five separate Tauri apps share one Rust core (`selfkit-core`) and one design system (`ui/`). Each device works with no server; the server only moves encrypted blobs between devices.

## Repo layout
| Path | What |
| --- | --- |
| `selfkit-core/` | Shared Rust library: crypto, keys, SQLite storage, migrations, sync, blobs, bundles, settings |
| `selfkit-agent/` | Desktop background service (no UI): activity tracking, sync, backups, reminders |
| `apps/{chrono,hisab,tickd,reflect,dusk}` | Tauri 2 apps |
| `ui/` | Shared Svelte components and design tokens |
| `extension/` | Chrome + Firefox WebExtension (native messaging → agent) |
| `android-plugins/` | Kotlin Tauri plugins (usage stats, jobs, ContentProvider) |
| `dev/` | Local fake server (Docker Postgres + MinIO) for tests |

## Data flow
```
 app UI (Svelte) ──Tauri cmd──▶ app Rust ──▶ selfkit-core::Store (SQLite, <data>/selfkit/<app>.db)
                                                    │ dirty rows
 selfkit-agent ──tracks──▶ chrono.db                ▼
        └── runs sync for all app DBs ──▶ selfkit-core::sync ──encrypt──▶ Postgres (sk_records) / S3 (blobs)
```
- **Shared data folder:** `dirs::data_dir()/selfkit/` holds one SQLite DB per app plus `settings.db`. Reflect and Dusk open other apps' DBs read-only.
- **App ↔ agent:** Unix socket `$XDG_RUNTIME_DIR/selfkit-agent.sock` (Linux), named pipe `\\.\pipe\selfkit-agent` (Windows). Line-delimited JSON. No network ports.
- **Android:** no agent. Sync/backups via WorkManager jobs, reminders via AlarmManager, cross-app sharing via a signature-protected ContentProvider.
