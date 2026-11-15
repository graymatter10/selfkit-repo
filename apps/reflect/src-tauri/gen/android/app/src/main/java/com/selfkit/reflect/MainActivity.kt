package com.selfkit.reflect

import android.app.Activity
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.WindowCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // ---- Selfkit sync (android-plugins/selfkit-sync): key for the secrets file before Rust starts.
    try {
      com.selfkit.sync.SelfkitSyncNative.ensureLoaded(this)
      com.selfkit.sync.SelfkitSyncNative.setWrapKey(com.selfkit.sync.SelfkitSecrets.wrapKey(this))
    } catch (t: Throwable) { android.util.Log.w("SelfkitSync", "wrap key", t) }
    // ---- end Selfkit sync
    super.onCreate(savedInstanceState)
    // ---- Selfkit sync: ~15 min job + sync on open (WorkManager).
    Thread { try { com.selfkit.sync.SelfkitSync.init(this) } catch (t: Throwable) { android.util.Log.w("SelfkitSync", "init", t) } }.start()
    // ---- end Selfkit sync
  }

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

  // window.ReflectBars: status/navigation bar icon colour follows the app theme.
  override fun onWebViewCreate(webView: WebView) {
    // ---- Selfkit sync: window.SelfkitSync for the shared Sync screen (battery tip, reschedule).
    webView.addJavascriptInterface(com.selfkit.sync.SelfkitSyncBridge(this), "SelfkitSync")
    // ---- end Selfkit sync
    webView.addJavascriptInterface(ReflectBars(this), "ReflectBars")
  }
}

class ReflectBars(private val act: Activity) {
  @JavascriptInterface
  fun setDark(dark: Boolean) {
    act.runOnUiThread {
      val c = WindowCompat.getInsetsController(act.window, act.window.decorView)
      c.isAppearanceLightStatusBars = !dark
      c.isAppearanceLightNavigationBars = !dark
    }
  }
}
