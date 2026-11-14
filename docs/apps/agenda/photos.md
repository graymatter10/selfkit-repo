# Agenda — photos in task descriptions

- Add in the task form: **Choose Photo** (gallery / file picker, several at once) and, on phones, **Take Photo** (camera, `capture="environment"`). Desktop shows **Add Photo…** (GTK file chooser).
- The webview scales each image to document quality: longest side ≤ 2400 px, WebP at quality 0.85 (both WebKitGTK and Android WebView encode WebP; JPEG is the fallback). EXIF orientation is applied.
- Rust (`photos.rs`) stores the bytes as `<data_dir>/agenda-photos/<id>.webp` (outside SQLite so the DB stays small) and a `photos` record with `file, mime, w, h, bytes, sha256, created_at`. Tasks list photo ids in `photos`.
- Details show the photos inside the notes card (grid). A photo can also be placed inline in the Markdown with `![caption](photo:<id>)`. Tap a photo for full screen; Back/Escape closes it.
- Removing a photo from a task keeps the file (undo stays possible). Not yet: cleanup of unused photo files, and syncing photo files between devices (planned via the S3 blob store; records already sync).
- Android camera: the manifest declares a `<queries>` entry for `IMAGE_CAPTURE`; no CAMERA permission is needed (the camera app takes the picture).
