package com.selfkit.hisab

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.provider.Telephony
import org.json.JSONArray
import org.json.JSONObject

/**
 * Keeps incoming SMS (only while the user has SMS import turned on) in a
 * small private queue. Hisab parses them into its review list the next time
 * it opens; nothing is saved as a transaction automatically.
 */
class SmsReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != Telephony.Sms.Intents.SMS_RECEIVED_ACTION) return
    val p = prefs(context)
    if (!p.getBoolean("enabled", false)) return
    val parts = Telephony.Sms.Intents.getMessagesFromIntent(intent) ?: return
    if (parts.isEmpty()) return
    val body = parts.joinToString("") { it.messageBody ?: "" }
    val sender = parts[0].originatingAddress ?: ""
    synchronized(LOCK) {
      val q = try { JSONArray(p.getString("queue", "[]")) } catch (_: Exception) { JSONArray() }
      q.put(JSONObject().put("sender", sender).put("body", body).put("at", parts[0].timestampMillis))
      while (q.length() > 200) q.remove(0)
      p.edit().putString("queue", q.toString()).apply()
    }
  }

  companion object {
    private val LOCK = Any()
    fun prefs(c: Context) = c.getSharedPreferences("hisab_sms", Context.MODE_PRIVATE)
    fun take(c: Context): String = synchronized(LOCK) {
      val p = prefs(c)
      val s = p.getString("queue", "[]") ?: "[]"
      p.edit().putString("queue", "[]").apply()
      s
    }
  }
}
