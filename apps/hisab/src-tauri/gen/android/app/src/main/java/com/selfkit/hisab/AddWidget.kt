package com.selfkit.hisab

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.Context
import android.content.Intent
import android.widget.RemoteViews

/** Home-screen "＋ Expense" button: opens Hisab on the number pad. */
class AddWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
    val intent = Intent(context, MainActivity::class.java).apply {
      action = ACTION_ADD
      flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
    }
    val pi = PendingIntent.getActivity(context, 1, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    for (id in ids) {
      val v = RemoteViews(context.packageName, R.layout.widget_add)
      v.setOnClickPendingIntent(R.id.widget_root, pi)
      manager.updateAppWidget(id, v)
    }
  }

  companion object {
    const val ACTION_ADD = "com.selfkit.hisab.ADD"
  }
}
