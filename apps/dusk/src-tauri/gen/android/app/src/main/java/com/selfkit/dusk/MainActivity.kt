package com.selfkit.dusk

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  // window.DuskUsage: Usage access + screen events for the sleep estimate,
  // and the status-bar icon colour (see DuskUsage.kt).
  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(DuskUsage(this), "DuskUsage")
  }
}
