# Daily notes with photos

For any day: a short text (≤ 1000 characters) and any number of photos. Shown on Today and on past days.

- **Empty state:** a row "Add a note about today" (or "about this day"). Filled: text (3 lines on phone, tap to edit) + square thumbnails; tapping a thumbnail opens it full size (Back/Escape/tap closes).
- **Editor:** full screen on phone (✕, date, Save; "Delete note" at the bottom), a dialog on desktop (Cancel / Save, Ctrl+Enter saves, Escape closes). Back on Android saves unsaved changes instead of asking. Deleting shows "Note deleted · Undo".
- **Photos:** picked with the system picker (camera or gallery on Android), scaled in the webview to document quality (longest side ≤ 2400 px, WebP at q 0.85, JPEG where the webview cannot encode WebP), sent to Rust as base64 and stored as a file in `<data_dir>/chrono-photos/` with a `photos` record (`file, mime, w, h, bytes, sha256, created_at`). Removing a photo from a note keeps the file (undo-safe); orphan clean-up is a later step.
- **Data:** `day_notes` record id `n-<YYYY-MM-DD>` `{day, text, photos: [ids], updated_at}`; saving empty text with no photos deletes it.
- **Code:** `src-tauri/src/notes.rs`, commands `note_get`, `note_save`, `photo_save`, `photo_get`; UI `components/NoteCard.svelte`, `NoteEditor.svelte`, `Photo.svelte`, `PhotoViewer.svelte`, `lib/photos.ts`.
- **Not yet:** photo bytes are not uploaded to S3 (records sync, files stay on the device that added them), like Agenda.
