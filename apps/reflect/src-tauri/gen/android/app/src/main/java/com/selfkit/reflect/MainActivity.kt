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
    super.onCreate(savedInstanceState)
  }

  // window.ReflectBars: status/navigation bar icon colour follows the app theme.
  override fun onWebViewCreate(webView: WebView) {
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
