# Conventions

- **Rust:** edition 2024, `cargo fmt`, `cargo clippy -- -D warnings` clean. Errors via `thiserror` in libraries (`selfkit_core::Error`), `anyhow` only in binaries.
- **Naming:** crates `selfkit-*`; collections are lowercase snake plural (`tasks`, `transactions`, `activity_events`).
- **Time:** store `i64` UTC milliseconds + `i32` tz offset minutes. Never store local time alone.
- **IDs:** UUIDv7 strings.
- **App identifiers:** `com.selfkit.{appname}` everywhere: Tauri `identifier`, Android `applicationId`/package, Windows AppUserModelID, keychain service. Apps: `com.selfkit.chrono`, `com.selfkit.hisab`, `com.selfkit.tickd`, `com.selfkit.reflect`, `com.selfkit.dusk`; agent `com.selfkit.agent` (also the browser native-messaging host name); shared keychain service `com.selfkit`.
- **Icons:** source SVGs in `ui/icons/<app>.svg` (1024×1024 artboard); generate platform sizes with `ui/icons/build.sh` → `apps/<app>/icons/` (Tauri reads `icons/icon.png` etc.).
- **Tests:** unit tests next to code; tests needing Postgres/S3 are `#[ignore]` and read `SELFKIT_TEST_PG` / `SELFKIT_TEST_S3_*` (see `dev/test.env`).
- **Commits:** imperative subject ≤ 72 chars; body says what docs changed.
- **Docs:** one topic per file, short, current. Each component/app doc ends with a code map: `path` — one line on what it does. Update STATUS.md and DECISIONS.md as you go.
