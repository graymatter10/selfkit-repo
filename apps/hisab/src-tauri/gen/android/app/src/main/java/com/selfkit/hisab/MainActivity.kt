package com.selfkit.hisab

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.provider.Telephony
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import org.json.JSONArray
import org.json.JSONObject

class MainActivity : TauriActivity() {
  @Volatile private var launchAction = ""

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    readAction(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    readAction(intent)
  }

  private fun readAction(i: Intent?) {
    if (i?.action == AddWidget.ACTION_ADD) launchAction = "add"
  }

  // window.HisabNative: status-bar colour, SMS import, widget launch.
  override fun onWebViewCreate(webView: WebView) {
    val act = this
    webView.addJavascriptInterface(object {
      @JavascriptInterface
      fun setDark(dark: Boolean) {
        runOnUiThread {
          val c = WindowCompat.getInsetsController(window, window.decorView)
          c.isAppearanceLightStatusBars = !dark
          c.isAppearanceLightNavigationBars = !dark
        }
      }

      @JavascriptInterface
      fun smsPermission(): Boolean =
        ContextCompat.checkSelfPermission(act, Manifest.permission.READ_SMS) == PackageManager.PERMISSION_GRANTED &&
          ContextCompat.checkSelfPermission(act, Manifest.permission.RECEIVE_SMS) == PackageManager.PERMISSION_GRANTED

      @JavascriptInterface
      fun requestSms() {
        runOnUiThread {
          ActivityCompat.requestPermissions(act, arrayOf(Manifest.permission.READ_SMS, Manifest.permission.RECEIVE_SMS), 77)
        }
      }

      @JavascriptInterface
      fun setSmsEnabled(on: Boolean) {
        SmsReceiver.prefs(act).edit().putBoolean("enabled", on).apply()
      }

      /** Inbox messages newer than sinceMs, newest first (max 400). */
      @JavascriptInterface
      fun readSms(sinceMs: Double): String {
        val out = JSONArray()
        if (!smsPermission()) return out.toString()
        try {
          contentResolver.query(
            Telephony.Sms.Inbox.CONTENT_URI,
            arrayOf(Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE),
            "${Telephony.Sms.DATE} > ?", arrayOf(sinceMs.toLong().toString()),
            "${Telephony.Sms.DATE} DESC",
          )?.use { c ->
            while (c.moveToNext() && out.length() < 400) {
              out.put(JSONObject().put("sender", c.getString(0) ?: "").put("body", c.getString(1) ?: "").put("at", c.getLong(2)))
            }
          }
        } catch (_: Exception) {}
        return out.toString()
      }

      /** Messages the receiver queued while the app was closed (cleared on read). */
      @JavascriptInterface
      fun takeQueued(): String = SmsReceiver.take(act)

      @JavascriptInterface
      fun minimize() {
        runOnUiThread { moveTaskToBack(true) }
      }

      @JavascriptInterface
      fun takeLaunchAction(): String {
        val a = launchAction
        launchAction = ""
        return a
      }
    }, "HisabNative")

    // ---- Selfkit photos (SelfkitPhotos.kt): camera/gallery choice, capture cleanup, Save to gallery.
    webView.addJavascriptInterface(SelfkitPhotos(this, "Hisab"), "SelfkitPhotos")
    // ---- end Selfkit photos
  }
}
