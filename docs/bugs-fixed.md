# Bug fixes (branch claude/batch-fixes-1655a1, 2026-11-20)

Fixes for bugs logged in `docs/bugs-linux.md` (L*), `docs/bugs-phone.md` (P*, lead B*) and `docs/bugs-code.md` (C*). Not built per fix; CI builds `test/fixes-*` copies of this branch.

| Bug | Fix | Files | Needs device retest |
|-----|-----|-------|---------------------|
| L1 | Starts/Ends picker opens on the day of its default time (next full hour / start + 1 h), not always today | `apps/agenda/src/components/WhenSheet.svelte` | Linux (set clock or wait until 23:xx) |
| B3 | PIN set-up sheet keeps message, Next and "Use a passphrase instead" rows on every step (hidden, not removed), so the pad stays put at "Repeat the PIN" | `ui/src/lock/LockSettings.svelte` | OPPO (all 5 apps share it) |
| B5 | Lock screen no longer keeps a fixed "Wrong PIN. Try again in 30 s." after the wait ends; the live countdown line shows the wait, then "Wrong PIN." | `ui/src/lock/LockGate.svelte` | OPPO (5 wrong PINs, wait, fingerprint cancel) |
