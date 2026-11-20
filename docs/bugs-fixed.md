# Bug fixes (branch claude/batch-fixes-1655a1, 2026-11-20)

Fixes for bugs logged in `docs/bugs-linux.md` (L*), `docs/bugs-phone.md` (P*, lead B*) and `docs/bugs-code.md` (C*). Not built per fix; CI builds `test/fixes-*` copies of this branch.

| Bug | Fix | Files | Needs device retest |
|-----|-----|-------|---------------------|
| L1 | Starts/Ends picker opens on the day of its default time (next full hour / start + 1 h), not always today | `apps/agenda/src/components/WhenSheet.svelte` | Linux (set clock or wait until 23:xx) |
| B3 | PIN set-up sheet keeps message, Next and "Use a passphrase instead" rows on every step (hidden, not removed), so the pad stays put at "Repeat the PIN" | `ui/src/lock/LockSettings.svelte` | OPPO (all 5 apps share it) |
| B5 | Lock screen no longer keeps a fixed "Wrong PIN. Try again in 30 s." after the wait ends; the live countdown line shows the wait, then "Wrong PIN." | `ui/src/lock/LockGate.svelte` | OPPO (5 wrong PINs, wait, fingerprint cancel) |

## Waiting for a product decision (lead asked)

**B2 — Reflect shows no Dusk sleep on phones.** On Android Reflect mirrors Dusk's *change records* (`SelfkitMirror`), but sleep comes from Dusk's local `dusk_nights` result cache (DECISIONS 2026-11-15: "Reflect reads the local `dusk_nights` cache instead of re-implementing the estimate"), which is not a record and never reaches the mirror. Options:
1. Dusk publishes each night as a synced record (e.g. `dusk_night` kind written next to the cache) so the mirror and sync carry it; Reflect reads those when the cache table is absent. Medium change (Dusk data.rs + Reflect sources.rs), keeps one estimator.
2. Dusk's provider serves the `dusk_nights` table as an extra read-only path; `SelfkitMirror` copies it. Kotlin + JNI change in all mirror code.
3. Keep as is; Reflect's Today/Settings say "Sleep from Dusk shows on the computer" on phones. Text-only.

**B4 — Dusk Settings "Tasks and spending · On computer" on the phone.** By design (docs/apps/dusk/README.md: "On Android these files are not on the device") Dusk on Android can't read Agenda/Hisab, so Connections' next-day cards stay muted even with both apps installed. Options:
1. Give Dusk the same read-only mirror Reflect has: manifest meta-data `com.selfkit.sync.MIRROR=agenda,hisab` + `SelfkitSync.mirror("agenda,hisab")` on open; `sources_status` then finds `<data dir>/agenda.db` / `hisab.db` and Connections work on the phone. Small change, needs the APK.
2. Keep desktop-only; change the label to "Desktop only" (and the cards' "isn't on this device" to the same), so it doesn't read as broken. Text-only.
