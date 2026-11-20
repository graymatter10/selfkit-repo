package com.selfkit.agenda

import android.content.ContentValues
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.MediaStore
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.WindowCompat

class MainActivity : TauriActivity() {
  // Release builds shrink resources; the notification plugin finds the small icon only by
  // name, so reference it here (P3; res/raw/selfkit_keep.xml keeps it too).
  @Suppress("unused") private val notificationIcon = R.drawable.ic_stat_agenda

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // ---- Selfkit sync (android-plugins/selfkit-sync): key for the secrets file before Rust starts.
    try {
      com.selfkit.sync.SelfkitSyncNative.ensureLoaded(this)
      com.selfkit.sync.SelfkitSyncNative.setWrapKey(com.selfkit.sync.SelfkitSecrets.wrapKey(this))
    } catch (t: Throwable) { android.util.Log.w("SelfkitSync", "wrap key", t) }
    // ---- end Selfkit sync
    super.onCreate(savedInstanceState)
    selfkitLock.onCreate() // ---- Selfkit lock: FLAG_SECURE before the first frame
    // ---- Selfkit sync: ~15 min job + sync on open (WorkManager).
    Thread { try { com.selfkit.sync.SelfkitSync.init(this) } catch (t: Throwable) { android.util.Log.w("SelfkitSync", "init", t) } }.start()
    // ---- end Selfkit sync
  }

  // ---- Selfkit lock (SelfkitLock.kt): fingerprint unlock, hide in app switcher, time away.
  private val selfkitLock by lazy { SelfkitLock(this) }

  override fun onStart() {
    super.onStart()
    selfkitLock.onStart()
  }

  override fun onStop() {
    selfkitLock.onStop()
    super.onStop()
  }
  // ---- end Selfkit lock

  // ---- Selfkit sync: LAN discovery broadcasts while the app is open.
  override fun onResume() {
    super.onResume()
    com.selfkit.sync.SelfkitSync.onForeground(this)
  }

  override fun onPause() {
    com.selfkit.sync.SelfkitSync.onBackground(this)
    super.onPause()
  }
  // ---- end Selfkit sync

  // The app can be set to Dark while the phone is in light mode (and the other
  // way round), so the page tells us which status-bar icon colour to use.
  override fun onWebViewCreate(webView: WebView) {
    // ---- Selfkit lock: window.SelfkitLock (biometric prompt, FLAG_SECURE, time away).
    selfkitLock.attach(webView)
    // ---- end Selfkit lock
    // ---- Selfkit sync: window.SelfkitSync for the shared Sync screen (battery tip, reschedule).
    webView.addJavascriptInterface(com.selfkit.sync.SelfkitSyncBridge(this), "SelfkitSync")
    // ---- end Selfkit sync
    webView.addJavascriptInterface(object {
      @JavascriptInterface
      fun setDark(dark: Boolean) {
        runOnUiThread {
          val c = WindowCompat.getInsetsController(window, window.decorView)
          c.isAppearanceLightStatusBars = !dark
          c.isAppearanceLightNavigationBars = !dark
        }
      }

      // Back with nothing open: leave the app like the system would (the page
      // handles Back itself through Tauri's back-button event).
      @JavascriptInterface
      fun toBackground() {
        runOnUiThread { moveTaskToBack(true) }
      }
    }, "AgendaBars")

    // .ics export: save a text file into the public Downloads folder.
    // Returns a readable location, or "" on failure.
    webView.addJavascriptInterface(object {
      @JavascriptInterface
      fun saveDownload(name: String, mime: String, text: String): String {
        return try {
          if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = ContentValues().apply {
              put(MediaStore.Downloads.DISPLAY_NAME, name)
              put(MediaStore.Downloads.MIME_TYPE, mime)
              put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val uri = contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: return ""
            contentResolver.openOutputStream(uri)?.use { it.write(text.toByteArray()) }
            values.clear()
            values.put(MediaStore.Downloads.IS_PENDING, 0)
            contentResolver.update(uri, values, null, null)
          } else {
            val dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
            dir.mkdirs()
            // Bare file name only: "../x" must not escape Downloads.
            java.io.File(dir, java.io.File(name).name.ifEmpty { "export.txt" }).writeText(text)
          }
          "Downloads/$name"
        } catch (e: Exception) {
          ""
        }
      }
    }, "AgendaFiles")

    // ---- Selfkit photos (SelfkitPhotos.kt): camera/gallery choice, capture cleanup, Save to gallery.
    webView.addJavascriptInterface(SelfkitPhotos(this, "Agenda"), "SelfkitPhotos")
    // ---- end Selfkit photos
  }
}
