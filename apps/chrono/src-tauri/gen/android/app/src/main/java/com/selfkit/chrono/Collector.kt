package com.selfkit.chrono

import android.app.AppOpsManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Process
import android.provider.Settings
import android.content.pm.ApplicationInfo
import android.graphics.Bitmap
import android.graphics.Canvas
import android.util.Base64
import java.io.ByteArrayOutputStream
import org.json.JSONArray
import org.json.JSONObject
import java.util.TimeZone

/**
 * Reads raw usage events since a cursor (UsageStatsManager, needs "Usage
 * access") plus the notification listener's hourly counts, and packs them
 * into one JSON batch for the Rust importer (src-tauri/src/android.rs).
 * Docs: docs/apps/chrono/android.md.
 */
object Collector {
  // UsageEvents.Event types the importer understands
  private val KEEP = setOf(1, 2, 12, 15, 16, 17, 18, 23, 26)

  fun usageGranted(ctx: Context): Boolean {
    val ops = ctx.getSystemService(Context.APP_OPS_SERVICE) as AppOpsManager
    val mode = if (Build.VERSION.SDK_INT >= 29) {
      ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), ctx.packageName)
    } else {
      @Suppress("DEPRECATION")
      ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), ctx.packageName)
    }
    return mode == AppOpsManager.MODE_ALLOWED
  }

  fun notificationsGranted(ctx: Context): Boolean {
    val flat = Settings.Secure.getString(ctx.contentResolver, "enabled_notification_listeners") ?: return false
    val me = ComponentName(ctx, ChronoNotificationListener::class.java)
    return flat.split(":").any { ComponentName.unflattenFromString(it) == me }
  }

  fun label(ctx: Context, pkg: String): String? = try {
    val pm = ctx.packageManager
    val info = if (Build.VERSION.SDK_INT >= 33) pm.getApplicationInfo(pkg, PackageManager.ApplicationInfoFlags.of(0)) else @Suppress("DEPRECATION") pm.getApplicationInfo(pkg, 0)
    pm.getApplicationLabel(info).toString()
  } catch (e: Exception) {
    null
  }

  private fun launchers(ctx: Context): List<String> {
    val i = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME)
    @Suppress("DEPRECATION")
    return ctx.packageManager.queryIntentActivities(i, 0).map { it.activityInfo.packageName }.distinct()
  }

  /**
   * Installed launchable apps: [package, label, firstInstallTime, lastUpdateTime, system].
   * Visible through the manifest <queries> launcher intent (no QUERY_ALL_PACKAGES).
   */
  private fun packages(ctx: Context): JSONArray {
    val pm = ctx.packageManager
    val i = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
    @Suppress("DEPRECATION")
    val pkgs = pm.queryIntentActivities(i, 0).map { it.activityInfo.packageName }.distinct()
    val out = JSONArray()
    for (p in pkgs) {
      try {
        @Suppress("DEPRECATION")
        val info = pm.getPackageInfo(p, 0)
        val ai = info.applicationInfo ?: continue
        val system = (ai.flags and ApplicationInfo.FLAG_SYSTEM) != 0 && (ai.flags and ApplicationInfo.FLAG_UPDATED_SYSTEM_APP) == 0
        out.put(JSONArray().put(p).put(pm.getApplicationLabel(ai).toString()).put(info.firstInstallTime).put(info.lastUpdateTime).put(system))
      } catch (e: Exception) {
        // uninstalled between the query and now
      }
    }
    return out
  }

  /** 48 px PNG data URL of an app icon (kept in android_apps so removed apps keep their icon). */
  private fun icon(ctx: Context, pkg: String): String? = try {
    val d = ctx.packageManager.getApplicationIcon(pkg)
    val bmp = Bitmap.createBitmap(48, 48, Bitmap.Config.ARGB_8888)
    val c = Canvas(bmp)
    d.setBounds(0, 0, 48, 48)
    d.draw(c)
    val out = ByteArrayOutputStream()
    bmp.compress(Bitmap.CompressFormat.PNG, 100, out)
    "data:image/png;base64," + Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
  } catch (e: Exception) {
    null
  }

  /** Icons only for packages not sent yet (or updated since), to keep batches small. */
  private fun newIcons(ctx: Context, pkgs: JSONArray): JSONObject {
    val prefs = ctx.getSharedPreferences("chrono_icons", Context.MODE_PRIVATE)
    val out = JSONObject()
    val ed = prefs.edit()
    for (k in 0 until pkgs.length()) {
      val row = pkgs.getJSONArray(k)
      val p = row.getString(0)
      val upd = row.getLong(3)
      if (prefs.getLong(p, -1) == upd) continue
      icon(ctx, p)?.let { out.put(p, it); ed.putLong(p, upd) }
    }
    ed.apply()
    return out
  }

  fun collect(ctx: Context, since: Long): String {
    val now = System.currentTimeMillis()
    val tz = TimeZone.getDefault()
    val events = JSONArray()
    val pkgs = HashSet<String>()
    if (usageGranted(ctx)) {
      val usm = ctx.getSystemService(Context.USAGE_STATS_SERVICE) as UsageStatsManager
      val ev = usm.queryEvents(since, now)
      val e = UsageEvents.Event()
      while (ev.hasNextEvent()) {
        ev.getNextEvent(e)
        val type = e.eventType
        if (type !in KEEP) continue
        val pkg = e.packageName ?: ""
        if (type == 1 || type == 12) pkgs.add(pkg)
        events.put(JSONArray().put(e.timeStamp).put(type).put(pkg).put(tz.getOffset(e.timeStamp) / 60000))
      }
    }
    val notif = JSONArray()
    for ((hour, pkg, n) in NotifStore.since(ctx, since)) {
      notif.put(JSONArray().put(hour).put(pkg).put(n))
      pkgs.add(pkg)
    }
    val labels = JSONObject()
    for (p in pkgs) label(ctx, p)?.let { labels.put(p, it) }
    val installed = packages(ctx)
    for (k in 0 until installed.length()) {
      val row = installed.getJSONArray(k)
      labels.put(row.getString(0), row.getString(1))
    }
    return JSONObject()
      .put("since", since)
      .put("now", now)
      .put("events", events)
      .put("labels", labels)
      .put("launchers", JSONArray(launchers(ctx)))
      .put("notif", notif)
      .put("tz_now", tz.getOffset(now) / 60000)
      .put("packages", installed)
      .put("icons", newIcons(ctx, installed))
      .toString()
  }
}
