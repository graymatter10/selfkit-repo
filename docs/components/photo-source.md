# Photo source sheet + Save to gallery (shared)

Used by every app that attaches photos: **Agenda** (task notes), **Hisab** (receipts, loan proof), **Chrono** (day notes). Dusk and Reflect attach no images, so they don't use it.

## What the user sees
- **Android**: tapping "Add photo" opens a small bottom sheet: **Take photo** ("Stays private in <App>, not in your gallery") · **Choose from gallery** · **Cancel**. Back, Cancel or tapping the dimmed area closes it and adds nothing. The sheet uses each app's own tokens (accent, card, ink, scrim), so it matches the app in light and dark.
  - *Take photo* opens the system camera right away. After the shutter and the camera's confirm, the photo goes through the app's normal import (≤ 2400 px, WebP) into the app's own photo folder, and the temporary camera file is deleted. Cancelling the camera adds nothing.
  - *Choose from gallery* opens the system photo picker, as before.
- **Desktop (Linux/Windows)**: no sheet. "Add photo" opens the normal file picker, as before. No webcam capture.
- **Photo viewer**: a **Save to gallery** button at the bottom (desktop: **Save to Pictures**, top centre so the bottom toast does not cover it). Android saves a copy into `Pictures/Selfkit/<App>/` (MediaStore, shows in the phone's gallery); desktop copies into `~/Pictures/Selfkit/<App>/` (override with `SELFKIT_PICTURES_DIR` for tests). A toast confirms ("Saved to gallery · Pictures/Selfkit/Agenda") and the button reads "Saved" for 2 s.

## How it works
- Both sheet choices click a hidden `<input type="file" accept="image/*">` that the app owns; the camera one has `capture="environment"`. Tauri's generated Android file chooser (`RustWebChromeClient`) turns a capture input into `ACTION_IMAGE_CAPTURE` with `EXTRA_OUTPUT` = a FileProvider URI for a `JPEG_*.jpg` temp file in `getExternalFilesDir(Pictures)` (the app's private external dir, `Android/data/<pkg>/files/Pictures`). MediaStore does not index that dir (and a `.nomedia` marker is written there), so the capture never shows in the gallery. The input's `File` then goes through the app's existing import path unchanged.
- **No `CAMERA` permission** is declared in any app: with it declared, the camera intent would need a runtime grant (Tauri's chooser checks `hasDefinedPermission`).
- Android 11+ package visibility: `resolveActivity(ACTION_IMAGE_CAPTURE)` returns null unless the manifest has `<queries><intent><action android:name="android.media.action.IMAGE_CAPTURE"/></intent></queries>`; without it Tauri silently falls back to the gallery picker (this was the "it always goes to the gallery" bug in Hisab and Chrono). All three manifests have it.
- After an import the app calls `cleanCaptures()` → `SelfkitPhotos.cleanCaptures()` deletes every `JPEG_*.jpg` in that dir. The bridge also runs it once when the webview is created, which removes a capture orphaned by process death (if Android kills the app while the camera is open, the result callback is lost; the next start just cleans up; nothing is added and nothing crashes).
- If no camera app exists (`hasCamera()` false) or there is no bridge (desktop), `askPhotoSource` opens the gallery picker straight away.

## Code map
- `ui/src/photo-source.svelte.ts`: `askPhotoSource({gallery, camera, cancel?})`, `choosePhotoSource`, `closePhotoSource()` (returns false when closed; apps call it first in their Back/Escape handler), `cleanCaptures()`, `saveToGallery(appName, data, desktop)`.
- `ui/src/components/PhotoSourceSheet.svelte`: the sheet (scoped CSS on app tokens, z-index 120). Mounted once in each `App.svelte` with `privateNote`.
- `ui/src/components/SavePhotoButton.svelte`: the viewer pill.
- Apps import these through `apps/<app>/src/lib/photo-source.ts` (relative path re-export; `@selfkit/ui` exports only its index and `ui/package.json` is unchanged).
- Kotlin: `apps/<app>/src-tauri/gen/android/app/src/main/java/com/selfkit/<app>/SelfkitPhotos.kt` (identical apart from the package), registered as `window.SelfkitPhotos` in a marked block at the end of `MainActivity.onWebViewCreate`. Methods: `hasCamera()`, `cleanCaptures()`, `saveToGallery(b64, mime, name)` (MediaStore `IS_PENDING` insert on Android 10+; Android 9 and older write to the public Pictures dir, which needs `WRITE_EXTERNAL_STORAGE` and otherwise fails with a toast).
- Rust (desktop save): `photo_export(id)` + `export_to_pictures` in `apps/{agenda,hisab,chrono}/src-tauri/src/lib.rs`.
- Per app: Agenda `TaskForm.svelte` (one "Add Photo" button; the old separate "Take Photo" button is gone), `App.svelte`, `PhotoViewer.svelte`; Hisab `lib/photo.ts` (`pickImage` → used by AddPanel, TxSheet, LoanSheet), `App.svelte`, `PhotoViewer.svelte`; Chrono `NoteEditor.svelte` (second hidden camera input), `App.svelte`, `PhotoViewer.svelte`.

## Known limits
- Android 9 and older: Save to gallery needs the storage permission, which the apps don't request (the user's phones are Android 12 and 15).
- Whether a particular camera app also keeps its own copy depends on that camera app; the stock cameras on the test phones don't (checked on the vivo, see the app READMEs).

## Tested (2026-11-15, vivo V2111 Android 12 + Linux)
- Agenda 0.4.2, Hisab 0.1.2, Chrono 0.1.1 on the vivo: Add photo → sheet → Back closes only the sheet; Take photo → system camera (`com.android.camera`) → shutter → OK → photo attached; MediaStore query shows no new image and the private capture dir holds only `.nomedia` afterwards; camera Cancel adds nothing and removes the empty temp file; Choose from gallery → system photo picker → picked image attached (Agenda); sheet Cancel from Hisab's edit sheet; Save to gallery → `Pictures/Selfkit/<App>/<App>_<date>.webp` in MediaStore + toast + "Saved"; process killed while the camera was open → app restarts normally, orphan capture removed. Dark (Agenda) and light (Chrono, Hisab).
- Chrono Back: sheet → note editor → app background, one layer per Back.
- Linux (Agenda debug build under Xvfb): Add Photo… opens the GTK file chooser directly (no sheet); Save to Pictures wrote the file and showed the toast over the viewer.
- Test items (task, receipt, note) and the gallery copies were deleted. The photo files themselves stay in each app's photo folder because deleting an item keeps its photos (undo; orphan cleanup is a known gap in all three apps).
