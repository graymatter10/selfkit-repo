package com.selfkit.chrono

import android.app.Notification
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification

/**
 * Opt-in notification counter (Settings → Notification access). Counts one
 * per new notification per app; updates of an existing notification, ongoing
 * ones (music, downloads), group summaries and our own are ignored. Nothing
 * about the content is read or stored.
 */
class ChronoNotificationListener : NotificationListenerService() {
  private val seen = HashMap<String, Long>()

  override fun onNotificationPosted(sbn: StatusBarNotification?) {
    sbn ?: return
    val n = sbn.notification ?: return
    if (sbn.packageName == packageName) return
    if (sbn.isOngoing || (n.flags and Notification.FLAG_GROUP_SUMMARY) != 0) return
    val prev = seen[sbn.key]
    if (prev != null && prev == n.`when`) return // an update of the same notification
    seen[sbn.key] = n.`when`
    if (seen.size > 500) seen.clear()
    NotifStore.add(applicationContext, sbn.packageName, System.currentTimeMillis())
  }

  override fun onNotificationRemoved(sbn: StatusBarNotification?) {
    sbn?.let { seen.remove(it.key) }
  }
}
