package com.selfkit.chrono

// ---- Selfkit shared photo helper (same file in agenda, hisab and chrono; only
// the package line differs). Docs: docs/components/photo-source.md.
// window.SelfkitPhotos:
//  - hasCamera(): a camera app can take ACTION_IMAGE_CAPTURE (needs the
//    <queries> IMAGE_CAPTURE entry in AndroidManifest.xml on Android 11+).
//  - cleanCaptures(): deletes the temporary JPEG_*.jpg files that Tauri's file
//    chooser writes for <input capture> into getExternalFilesDir(Pictures)
//    (app-private, kept out of the gallery with a .nomedia marker). Also run
//    once at start, which clears a capture orphaned by process death.
//  - saveToGallery(b64, mime, name): "Save to gallery" from the photo viewer,
//    MediaStore insert into Pictures/Selfkit/<App>. Returns the relative
//    folder/name, or "" on failure.
// No CAMERA permission is declared, so the camera intent needs no runtime grant.

import android.app.Activity
import android.content.ContentValues
import android.content.Intent
import android.media.MediaScannerConnection
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import android.webkit.JavascriptInterface
import java.io.File
import java.io.IOException

class SelfkitPhotos(private val act: Activity, private val appName: String) {
  init {
    Thread { cleanCaptures() }.start()
  }

  @JavascriptInterface
  fun hasCamera(): Boolean =
    try {
      Intent(MediaStore.ACTION_IMAGE_CAPTURE).resolveActivity(act.packageManager) != null
    } catch (_: Exception) {
      false
    }

  @JavascriptInterface
  fun cleanCaptures(): Int {
    var n = 0
    try {
      val dir = act.getExternalFilesDir(Environment.DIRECTORY_PICTURES) ?: return 0
      dir.mkdirs()
      val marker = File(dir, ".nomedia")
      if (!marker.exists()) marker.createNewFile()
      dir.listFiles()?.forEach { f ->
        if (f.isFile && f.name.startsWith("JPEG_") && f.name.endsWith(".jpg") && f.delete()) n++
      }
    } catch (_: Exception) {}
    return n
  }

  @JavascriptInterface
  fun saveToGallery(b64: String, mime: String, name: String): String {
    val folder = "${Environment.DIRECTORY_PICTURES}/Selfkit/$appName"
    return try {
      val bytes = Base64.decode(b64, Base64.DEFAULT)
      if (bytes.isEmpty()) return ""
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        val cr = act.contentResolver
        val values = ContentValues().apply {
          put(MediaStore.Images.Media.DISPLAY_NAME, name)
          put(MediaStore.Images.Media.MIME_TYPE, mime)
          put(MediaStore.Images.Media.RELATIVE_PATH, folder)
          put(MediaStore.Images.Media.IS_PENDING, 1)
        }
        val uri = cr.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values) ?: return ""
        try {
          (cr.openOutputStream(uri) ?: throw IOException("no stream")).use { it.write(bytes) }
        } catch (e: Exception) {
          cr.delete(uri, null, null)
          throw e
        }
        values.clear()
        values.put(MediaStore.Images.Media.IS_PENDING, 0)
        cr.update(uri, values, null, null)
      } else {
        // Android 9 and older: needs WRITE_EXTERNAL_STORAGE; fails cleanly without it.
        @Suppress("DEPRECATION")
        val dir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES), "Selfkit/$appName")
        dir.mkdirs()
        val f = File(dir, name)
        f.writeBytes(bytes)
        MediaScannerConnection.scanFile(act, arrayOf(f.path), arrayOf(mime), null)
      }
      "$folder/$name"
    } catch (_: Exception) {
      ""
    }
  }
}
// ---- end Selfkit shared photo helper
