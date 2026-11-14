package com.selfkit.dusk

import android.app.Activity
import android.app.AppOpsManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Process
import android.provider.Settings
import android.webkit.JavascriptInterface
import androidx.core.view.WindowCompat

/**
 * Dusk's small Android bridge, exposed to the page as `window.DuskUsage`
 * (docs/apps/dusk/android.md). Dusk can't rely on Chrono being installed, so
 * it reads UsageStatsManager itself:
 *  - hasAccess(): is "Usage access" granted for Dusk?
 *  - openAccessSettings(): open the system Usage access screen (the user grants it there).
 *  - events(from, to): JSON `[[timeMs, type], ...]` of screen and app events
 *    (types 1 resumed, 2 paused, 15 screen interactive, 16 non-interactive,
 *    17 keyguard shown, 18 keyguard hidden). Rust turns them into use intervals.
 *  - setDark(dark): status/navigation bar icon colour follows the app theme.
 * Extension point (later): Health Connect sleep sessions would be read here too.
 */
class DuskUsage(private val act: Activity) {
  private val types = setOf(1, 2, 15, 16, 17, 18)

  @JavascriptInterface
  fun hasAccess(): Boolean {
    val ops = act.getSystemService(Context.APP_OPS_SERVICE) as AppOpsManager
    val mode = if (Build.VERSION.SDK_INT >= 29)
      ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), act.packageName)
    else
      @Suppress("DEPRECATION") ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), act.packageName)
    if (mode == AppOpsManager.MODE_DEFAULT) {
      return act.checkCallingOrSelfPermission(android.Manifest.permission.PACKAGE_USAGE_STATS) == android.content.pm.PackageManager.PERMISSION_GRANTED
    }
    return mode == AppOpsManager.MODE_ALLOWED
  }

  @JavascriptInterface
  fun openAccessSettings() {
    act.runOnUiThread {
      try {
        act.startActivity(Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS, Uri.parse("package:" + act.packageName)))
      } catch (e: Exception) {
        act.startActivity(Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS))
      }
    }
  }

  @JavascriptInterface
  fun events(from: Long, to: Long): String {
    if (!hasAccess()) return "[]"
    val usm = act.getSystemService(Context.USAGE_STATS_SERVICE) as UsageStatsManager
    val it = usm.queryEvents(from, to) ?: return "[]"
    val e = UsageEvents.Event()
    val sb = StringBuilder("[")
    var first = true
    while (it.hasNextEvent()) {
      it.getNextEvent(e)
      if (e.eventType in types) {
        if (!first) sb.append(',')
        sb.append('[').append(e.timeStamp).append(',').append(e.eventType).append(']')
        first = false
      }
    }
    return sb.append(']').toString()
  }

  @JavascriptInterface
  fun setDark(dark: Boolean) {
    act.runOnUiThread {
      val c = WindowCompat.getInsetsController(act.window, act.window.decorView)
      c.isAppearanceLightStatusBars = !dark
      c.isAppearanceLightNavigationBars = !dark
    }
  }
}
