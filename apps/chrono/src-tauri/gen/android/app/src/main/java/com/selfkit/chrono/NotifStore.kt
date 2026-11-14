package com.selfkit.chrono

import android.content.Context
import org.json.JSONObject

/**
 * Hourly notification counts per app, kept in SharedPreferences until the
 * importer picks them up: key "<hour start UTC ms>|<package>" → count.
 * Only counts are stored, never titles, senders or text.
 */
object NotifStore {
  private const val PREFS = "chrono_notif"
  private const val KEY = "counts"
  private const val HOUR = 3_600_000L
  private const val KEEP_MS = 8 * 24 * HOUR

  @Synchronized
  fun add(ctx: Context, pkg: String, at: Long) {
    val p = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    val o = JSONObject(p.getString(KEY, "{}") ?: "{}")
    val hour = at / HOUR * HOUR
    val k = "$hour|$pkg"
    o.put(k, o.optLong(k, 0) + 1)
    // prune old hours
    val cut = at - KEEP_MS
    val it = o.keys()
    val drop = ArrayList<String>()
    while (it.hasNext()) {
      val key = it.next()
      if ((key.substringBefore('|').toLongOrNull() ?: 0) < cut) drop.add(key)
    }
    drop.forEach { o.remove(it) }
    p.edit().putString(KEY, o.toString()).apply()
  }

  @Synchronized
  fun since(ctx: Context, since: Long): List<Triple<Long, String, Long>> {
    val o = JSONObject(ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, "{}") ?: "{}")
    val out = ArrayList<Triple<Long, String, Long>>()
    val it = o.keys()
    while (it.hasNext()) {
      val key = it.next()
      val hour = key.substringBefore('|').toLongOrNull() ?: continue
      if (hour + HOUR <= since) continue
      out.add(Triple(hour, key.substringAfter('|'), o.optLong(key, 0)))
    }
    return out
  }
}
