package com.selfkit.sync

import android.annotation.SuppressLint
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.work.Constraints
import androidx.work.Data
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * App-facing API of the Android side. Typical use (MainActivity):
 *   onCreate:  SelfkitSync.init(this)      // wrap key -> Rust, schedule job, sync on open
 *   onResume:  SelfkitSync.onForeground(this)
 *   onPause:   SelfkitSync.onBackground(this)
 * The app's Rust code calls selfkit-appsync directly for status / sync now / settings / conflicts;
 * this class adds what only Android can do: WorkManager, MulticastLock, battery screens.
 */
object SelfkitSync {
    private const val PERIODIC = "selfkit-sync-periodic"
    private const val ONCE = "selfkit-sync-once"
    private var foregroundLock: WifiManager.MulticastLock? = null

    private fun deviceName(): String {
        val m = Build.MANUFACTURER.replaceFirstChar { it.uppercase() }
        return if (Build.MODEL.startsWith(Build.MANUFACTURER, true)) Build.MODEL else "$m ${Build.MODEL}"
    }

    /** Raw JSON call into selfkit-appsync (status, sync_now, settings, set_settings, schedule, conflicts, resolve, changes, put). */
    fun call(ctx: Context, method: String, args: JSONObject = JSONObject()): JSONObject {
        SelfkitSyncNative.ensureLoaded(ctx)
        val out = SelfkitSyncNative.call(Meta.app(ctx), Meta.dataDir(ctx), SelfkitSecrets.wrapKey(ctx), deviceName(), method, args.toString())
        return if (out.trimStart().startsWith("[")) JSONObject().put("items", org.json.JSONArray(out)) else JSONObject(out)
    }

    fun init(ctx: Context) {
        SelfkitSyncNative.ensureLoaded(ctx)
        SelfkitSyncNative.setWrapKey(SelfkitSecrets.wrapKey(ctx))
        schedule(ctx)
        val s = call(ctx, "schedule")
        if (s.optBoolean("run_on_open")) runOnce(ctx, "open")
    }

    /** (Re)apply the periodic job from the Rust settings (owner, auto sync, minutes). Call after settings change. */
    fun schedule(ctx: Context) {
        val s = call(ctx, "schedule")
        val wm = WorkManager.getInstance(ctx)
        if (!s.optBoolean("enabled")) {
            wm.cancelUniqueWork(PERIODIC)
            return
        }
        val minutes = s.optLong("every_minutes", 15).coerceAtLeast(15)
        val req = PeriodicWorkRequestBuilder<SyncWorker>(minutes, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setInputData(Data.Builder().putString(SyncWorker.KEY_TRIGGER, "background").build())
            .build()
        wm.enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.UPDATE, req)
    }

    /** Fire-and-forget pass in the background (on open, or "Sync now" without waiting). */
    fun runOnce(ctx: Context, trigger: String = "manual") {
        val req = OneTimeWorkRequestBuilder<SyncWorker>()
            .setInputData(Data.Builder().putString(SyncWorker.KEY_TRIGGER, trigger).build())
            .build()
        WorkManager.getInstance(ctx).enqueueUniqueWork(ONCE, ExistingWorkPolicy.KEEP, req)
    }

    /** One pass on the calling thread (never the main thread), with a MulticastLock held. */
    fun syncBlocking(ctx: Context, trigger: String = "manual"): JSONObject {
        val lock = multicastLock(ctx, "selfkit-sync-pass")
        try {
            lock?.acquire()
            return call(ctx, "sync_now", JSONObject().put("trigger", trigger))
        } finally {
            if (lock?.isHeld == true) lock.release()
        }
    }

    /** While the app is open, LAN beacons can arrive (for passes run from the app's Rust code). */
    fun onForeground(ctx: Context) {
        if (foregroundLock == null) foregroundLock = multicastLock(ctx, "selfkit-sync-foreground")
        foregroundLock?.let { if (!it.isHeld) it.acquire() }
    }

    fun onBackground(@Suppress("UNUSED_PARAMETER") ctx: Context) {
        foregroundLock?.let { if (it.isHeld) it.release() }
    }

    private fun multicastLock(ctx: Context, tag: String): WifiManager.MulticastLock? {
        val wifi = ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return null
        return wifi.createMulticastLock(tag).apply { setReferenceCounted(false) }
    }

    // ---- battery optimisation hint (OPPO/ColorOS, vivo/Funtouch kill background jobs) ----

    fun isIgnoringBatteryOptimizations(ctx: Context): Boolean {
        val pm = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager
        return pm.isIgnoringBatteryOptimizations(ctx.packageName)
    }

    /** True on phones known to stop periodic jobs unless the user allows background activity. */
    fun needsBatteryTip(ctx: Context): Boolean {
        val m = Build.MANUFACTURER.lowercase()
        return !isIgnoringBatteryOptimizations(ctx) && listOf("oppo", "realme", "oneplus", "vivo", "iqoo", "xiaomi", "huawei", "honor").any { m.contains(it) }
    }

    /** Best screen for "Allow background activity": vendor screen if it exists, else Android's own. */
    @SuppressLint("BatteryLife")
    fun batteryIntent(ctx: Context): Intent {
        val pkg = ctx.packageName
        val vendor = listOf(
            ComponentName("com.coloros.safecenter", "com.coloros.safecenter.permission.startup.StartupAppListActivity"),
            ComponentName("com.oplus.battery", "com.oplus.powermanager.fuelgaue.PowerUsageModelActivity"),
            ComponentName("com.vivo.permissionmanager", "com.vivo.permissionmanager.activity.BgStartUpManagerActivity"),
            ComponentName("com.iqoo.secure", "com.iqoo.secure.ui.phoneoptimize.BgStartUpManager"),
        )
        for (c in vendor) {
            val i = Intent().setComponent(c).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            if (i.resolveActivity(ctx.packageManager) != null) return i
        }
        val direct = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$pkg")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (direct.resolveActivity(ctx.packageManager) != null) return direct
        return Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$pkg")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
}
