package com.selfkit.agenda

// ---- Selfkit lock (docs/components/app-lock.md). Same file in every app; only the package differs.
// Android side of the shared app lock (ui/src/lock): fingerprint/face unlock through
// BiometricPrompt (phone screen lock as fallback), "Hide content in app switcher"
// (FLAG_SECURE) and how long the app was away (stopped -> started), which the page hands
// to Rust (lock_away). The PIN itself never reaches Kotlin.

import android.os.Build
import android.os.SystemClock
import android.view.WindowManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.appcompat.app.AppCompatActivity
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import org.json.JSONObject

class SelfkitLock(private val act: AppCompatActivity) {
  private var webView: WebView? = null
  @Volatile private var away = -1L
  @Volatile private var authBusy = false
  private val prefs by lazy { act.getSharedPreferences("selfkit_lock", 0) }

  /** MainActivity.onCreate: apply FLAG_SECURE before the first frame. */
  fun onCreate() {
    applySecure(prefs.getBoolean("secure", false))
  }

  /** MainActivity.onWebViewCreate: window.SelfkitLock. */
  fun attach(w: WebView) {
    webView = w
    w.addJavascriptInterface(this, "SelfkitLock")
  }

  fun onStop() {
    // The phone's own credential screen stops this activity: not "away".
    if (!authBusy) stoppedAt = SystemClock.elapsedRealtime()
  }

  fun onStart() {
    val s = stoppedAt
    if (s == 0L || authBusy) return
    stoppedAt = 0L
    away = SystemClock.elapsedRealtime() - s
    emit("selfkit-lock-resume", away.toString())
  }

  /** Time away (ms) since the last stop, once; -1 when none. */
  @JavascriptInterface
  fun takeAway(): Long {
    val a = away
    away = -1L
    return a
  }

  private fun authenticators(): Int =
    if (Build.VERSION.SDK_INT >= 30) BIOMETRIC_STRONG or DEVICE_CREDENTIAL else BIOMETRIC_STRONG

  /** "fingerprint" (strong biometric enrolled), "credential" (only the phone's screen lock),
   *  "none" (nothing set up) or "unavailable". */
  @JavascriptInterface
  fun biometricStatus(): String {
    return try {
      val m = BiometricManager.from(act)
      if (m.canAuthenticate(BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS) return "fingerprint"
      when (m.canAuthenticate(authenticators())) {
        BiometricManager.BIOMETRIC_SUCCESS -> "credential"
        BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> "none"
        else -> "unavailable"
      }
    } catch (t: Throwable) {
      "unavailable"
    }
  }

  /** Shows the system prompt; the answer arrives as the window event "selfkit-lock-auth"
   *  with detail "ok", "cancel" or "error:<message>". */
  @JavascriptInterface
  fun authenticate(title: String, subtitle: String) {
    act.runOnUiThread {
      try {
        authBusy = true
        val prompt = BiometricPrompt(act, ContextCompat.getMainExecutor(act), object : BiometricPrompt.AuthenticationCallback() {
          override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = done("ok")
          override fun onAuthenticationError(errorCode: Int, errString: CharSequence) = done(
            when (errorCode) {
              BiometricPrompt.ERROR_USER_CANCELED, BiometricPrompt.ERROR_NEGATIVE_BUTTON, BiometricPrompt.ERROR_CANCELED -> "cancel"
              else -> "error:$errString"
            }
          )
        })
        val info = BiometricPrompt.PromptInfo.Builder().setTitle(title).setConfirmationRequired(false)
        if (subtitle.isNotEmpty()) info.setSubtitle(subtitle)
        info.setAllowedAuthenticators(authenticators())
        // Before Android 11 the phone's screen lock can't be combined with strong biometrics.
        if (Build.VERSION.SDK_INT < 30) info.setNegativeButtonText("Use PIN")
        prompt.authenticate(info.build())
      } catch (t: Throwable) {
        done("error:${t.message ?: "biometric prompt failed"}")
      }
    }
  }

  private companion object {
    // elapsedRealtime counts deep sleep too, and can't be moved by changing the clock.
    // Process-level, so an Activity recreated while in the background still knows when
    // the app was stopped and locks on return (C5).
    @Volatile var stoppedAt = 0L
  }

  private fun done(result: String) {
    authBusy = false
    stoppedAt = 0L
    emit("selfkit-lock-auth", result)
  }

  /** Hide content in the app switcher (and block screenshots). Remembered for the next start. */
  @JavascriptInterface
  fun setSecure(on: Boolean) {
    prefs.edit().putBoolean("secure", on).apply()
    act.runOnUiThread { applySecure(on) }
  }

  private fun applySecure(on: Boolean) {
    if (on) act.window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    else act.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
  }

  private fun emit(name: String, detail: String) {
    val w = webView ?: return
    val js = "window.dispatchEvent(new CustomEvent(${JSONObject.quote(name)},{detail:${JSONObject.quote(detail)}}))"
    w.post { w.evaluateJavascript(js, null) }
  }
}
// ---- end Selfkit lock
