package com.selfkit.chrono

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Periodic background import (every 15 min, WorkManager): usage events and
 * notification counts → chrono.db through the Rust importer, then shows any
 * goal/limit or weekly-report alerts it returns.
 */
class ImportWorker(ctx: Context, params: WorkerParameters) : Worker(ctx, params) {
  override fun doWork(): Result {
    val ctx = applicationContext
    if (!Collector.usageGranted(ctx) && !Collector.notificationsGranted(ctx)) return Result.success()
    return try {
      val dir = ctx.dataDir.absolutePath
      val since = ChronoNative.cursor(dir)
      if (since < 0) return Result.retry()
      val res = JSONObject(ChronoNative.ingest(dir, Collector.collect(ctx, since)))
      if (!res.optBoolean("ok")) {
        Log.w(TAG, "import failed: ${res.optString("error")}")
        return Result.retry()
      }
      val alerts = res.optJSONArray("alerts")
      if (alerts != null) for (i in 0 until alerts.length()) {
        val a = alerts.getJSONObject(i)
        notify(ctx, a.optString("id"), a.optString("title"), a.optString("body"))
      }
      Result.success()
    } catch (e: Throwable) {
      Log.w(TAG, "import error", e)
      Result.retry()
    }
  }

  companion object {
    private const val TAG = "ChronoImport"
    const val CHANNEL = "chrono_alerts"
    private const val WORK = "chrono-import"

    fun schedule(ctx: Context) {
      val req = PeriodicWorkRequestBuilder<ImportWorker>(15, TimeUnit.MINUTES).build()
      WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, req)
    }

    fun notify(ctx: Context, id: String, title: String, body: String) {
      if (Build.VERSION.SDK_INT >= 33 && ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
      val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
      if (Build.VERSION.SDK_INT >= 26 && nm.getNotificationChannel(CHANNEL) == null) {
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Goals, limits and weekly report", NotificationManager.IMPORTANCE_DEFAULT))
      }
      val open = PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), PendingIntent.FLAG_IMMUTABLE)
      val n = NotificationCompat.Builder(ctx, CHANNEL)
        .setSmallIcon(R.drawable.ic_stat_chrono)
        .setContentTitle(title)
        .setContentText(body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(body))
        .setContentIntent(open)
        .setAutoCancel(true)
        .build()
      NotificationManagerCompat.from(ctx).notify(id.hashCode(), n)
    }
  }
}
