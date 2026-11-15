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

  // window.DuskUsage: Usage access + screen events for the sleep estimate,
  // and the status-bar icon colour (see DuskUsage.kt).
  override fun onWebViewCreate(webView: WebView) {
    // ---- Selfkit lock: window.SelfkitLock (biometric prompt, FLAG_SECURE, time away).
    selfkitLock.attach(webView)
    // ---- end Selfkit lock
    // ---- Selfkit sync: window.SelfkitSync for the shared Sync screen (battery tip, reschedule).
    webView.addJavascriptInterface(com.selfkit.sync.SelfkitSyncBridge(this), "SelfkitSync")
    // ---- end Selfkit sync
    webView.addJavascriptInterface(DuskUsage(this), "DuskUsage")
  }
}
