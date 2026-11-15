# Cutting a release

Everything is built in the cloud by `.github/workflows/release.yml`; nothing heavy runs on the dev machine. The result is a **draft** GitHub Release with every installer attached. Nothing is public until you press Publish.

## What it builds
- `android` ×5 (`android.yml`, one per app): release APK, arm64.
- `windows` (`windows.yml`): per-app NSIS installers, agent installer, `SelfkitSetup_<v>_x64.exe`; tested end to end ([platforms/windows.md](platforms/windows.md)).
- `linux` (`linux.yml`): per-app `.deb` + `.AppImage`, agent `.deb` + tarball; tested ([platforms/linux.md](platforms/linux.md)).
- `publish`: collects all files, signs the APKs (if the signing secrets exist), writes `SHA256SUMS.txt` and release notes (app versions table), and creates the draft.

## Steps
1. Bump versions: each app's `apps/<app>/src-tauri/tauri.conf.json` `version` (installer names use it), and `[workspace.package] version` in `Cargo.toml` for the agent. The suite version (Selfkit Setup's name, release title) comes from the tag.
2. Optional dry run first (builds and tests everything, creates no release): `installer/ci-installers.sh release`. It pushes a snapshot of the working tree to `ci/installers-release/<stamp>`, waits, downloads the files to `/shared/selfkitextra/installers/<stamp>/` and deletes the branch.
3. Commit, then tag and push: `git tag v0.2.0 && git push origin v0.2.0`. Or run it by hand: Actions → release → Run workflow (inputs: `version`, `test`, `draft`).
4. Wait for the run (~40–60 min; the jobs run in parallel). Check the draft under Releases: file list, notes, checksums.
5. Publish the draft (GitHub creates the tag on publish for hand-started runs).

## Test key off in release builds
`release.yml` passes `release: true` to the android/linux/windows workflows, which compile with `SELFKIT_RELEASE=1`: the public sync test key and test server are disabled in those builds (apps still list `sync-test-key` in their default features for test builds). Local and `dev/ci-apk.sh` builds keep the test key. See [security.md](security.md#test-key-test-builds-only).

## APK signing
The phones have the apps signed with the local debug key (`$ANDROID_USER_HOME/debug.keystore`, alias `androiddebugkey`), and Android only accepts updates signed with the same key. To have `release.yml` sign, add three repository secrets (Settings → Secrets and variables → Actions):
- `ANDROID_KEYSTORE_B64`: `base64 -w0 <keystore>`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`

Without them the draft carries `<app>-<v>-arm64-unsigned.apk`; sign locally with the zipalign + apksigner steps in `dev/ci-apk.sh`.

## Notes
- Windows installers are not code-signed, so SmartScreen warns on first run ("More info → Run anyway").
- Test-only snapshots: `installer/ci-installers.sh` (Windows only; Linux builds locally with `installer/linux/build-local.sh`) and `dev/ci-apk.sh <app>` (one APK). Branches `ci/installers/**` trigger windows only, `ci/installers-release/**` the release dry run, other `ci/<app>/**` one APK.
