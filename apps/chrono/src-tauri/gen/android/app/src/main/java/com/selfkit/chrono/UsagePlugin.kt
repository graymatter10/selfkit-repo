package com.selfkit.chrono

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class CollectArgs {
  var since: Long = 0
}

@InvokeArg
class OpenArgs {
  var which: String = "usage"
}

/**
 * Tauri plugin called from Rust (src-tauri/src/lib.rs, module mobile):
 * permission status, opening the system settings pages, and collecting a
 * usage batch. Also schedules the 15-minute background import.
 */
@TauriPlugin
class UsagePlugin(private val activity: Activity) : Plugin(activity) {
  override fun load(webView: android.webkit.WebView) {
    super.load(webView)
    ImportWorker.schedule(activity.applicationContext)
  }

  @Command
  fun status(invoke: Invoke) {
    val r = JSObject()
    r.put("usage", Collector.usageGranted(activity))
    r.put("notifications", Collector.notificationsGranted(activity))
    invoke.resolve(r)
  }

  @Command
  fun open(invoke: Invoke) {
    val args = invoke.parseArgs(OpenArgs::class.java)
    val pkg = Uri.parse("package:" + activity.packageName)
    val intent = if (args.which == "notifications") {
      if (android.os.Build.VERSION.SDK_INT >= 30) {
        Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS).putExtra(
          Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME,
          android.content.ComponentName(activity, ChronoNotificationListener::class.java).flattenToString(),
        )
      } else Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
    } else {
      Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS, pkg)
    }
    try {
      activity.startActivity(intent)
    } catch (e: Exception) {
      // some devices do not support the per-app page: open the list instead
      activity.startActivity(Intent(if (args.which == "notifications") Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS else Settings.ACTION_USAGE_ACCESS_SETTINGS))
    }
    invoke.resolve(JSObject())
  }

  @Command
  fun collect(invoke: Invoke) {
    val args = invoke.parseArgs(CollectArgs::class.java)
    val r = JSObject()
    if (!Collector.usageGranted(activity) && !Collector.notificationsGranted(activity)) {
      r.put("granted", false)
      invoke.resolve(r)
      return
    }
    Thread {
      try {
        r.put("granted", true)
        r.put("payload", Collector.collect(activity.applicationContext, args.since))
        invoke.resolve(r)
      } catch (e: Exception) {
        invoke.reject(e.message ?: "collect failed")
      }
    }.start()
  }
}
