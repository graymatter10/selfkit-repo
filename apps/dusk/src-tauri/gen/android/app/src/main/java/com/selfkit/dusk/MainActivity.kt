package com.selfkit.dusk

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

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

  // window.DuskUsage: Usage access + screen events for the sleep estimate,
  // and the status-bar icon colour (see DuskUsage.kt).
  override fun onWebViewCreate(webView: WebView) {
    // ---- Selfkit sync: window.SelfkitSync for the shared Sync screen (battery tip, reschedule).
    webView.addJavascriptInterface(com.selfkit.sync.SelfkitSyncBridge(this), "SelfkitSync")
    // ---- end Selfkit sync
    webView.addJavascriptInterface(DuskUsage(this), "DuskUsage")
  }
}
