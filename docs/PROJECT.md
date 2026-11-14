# Project

Five personal apps for one user on one Android phone, one Windows PC and one Linux PC (i3/X11). Full intent: [PLANNING.md](PLANNING.md).

| App | Purpose |
| --- | --- |
| Chrono | Activity on phone, PCs and browsers: apps, sites, time, peaks, trends |
| Hisab | Money: income, spending, budgets, loans |
| Tickd | Tasks with descriptions, photos, reminders, CalDAV sync |
| Reflect | Read-only stats dashboard from Chrono, Hisab, Dusk |
| Dusk | Sleep: estimated from device use + manual log |

## Constraints
- 300–500 MB RAM per app on weak devices; release builds with LTO + strip.
- Fully offline; server only syncs. No CDN assets.
- Everything encrypted on device before upload (XChaCha20-Poly1305).
- Any Postgres (standard SQL only), any S3-compatible storage. Start: Neon + Backblaze B2.
- Distraction-free: each app opens straight into its one job. No feeds, badges, streak pop-ups.
- English only, default currency USD. Timestamps in UTC + local offset.

## Build order
1. selfkit-core · 2. agent + Chrono tracker, extension, Android usage reader · 3. Tickd · 4. Hisab · 5. Chrono dashboard · 6. Reflect, Dusk.
