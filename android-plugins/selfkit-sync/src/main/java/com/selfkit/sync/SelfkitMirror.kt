package com.selfkit.sync

import android.content.Context
import android.net.Uri
import org.json.JSONArray
import org.json.JSONObject

/**
 * Read-only copies of other Selfkit apps' data on this phone (e.g. Reflect reading
 * Chrono, Hisab, Agenda, Dusk). Pulls `content://com.selfkit.<app>.provider/changes`
 * (signature permission com.selfkit.permission.SHARE) page by page and stores the
 * records in `<this app's data dir>/<app>.db`, the same file name the desktop uses,
 * so the app's existing read code works unchanged. Incremental (cursor per app).
 */
object SelfkitMirror {
    /** Returns {"chrono": {"ok":true,"records":12}, "hisab": {"ok":false,"error":"not installed"}, ...}. */
    fun pull(ctx: Context, apps: List<String>): JSONObject {
        val out = JSONObject()
        for (app in apps) {
            out.put(app, try { pullOne(ctx, app) } catch (t: Throwable) {
                JSONObject().put("ok", false).put("error", t.message ?: t.javaClass.simpleName)
            })
        }
        return out
    }

    private fun pullOne(ctx: Context, app: String): JSONObject {
        val authority = "com.selfkit.$app.provider"
        if (ctx.packageManager.resolveContentProvider(authority, 0) == null) {
            return JSONObject().put("ok", false).put("error", "not installed")
        }
        var since = SelfkitSync.call(ctx, "mirror_cursor", JSONObject().put("store", app)).optLong("cursor", 0)
        var total = 0
        while (true) {
            val uri = Uri.parse("content://$authority/changes?store=$app&since=$since&limit=500")
            val records = JSONArray()
            var upto = since
            ctx.contentResolver.query(uri, null, null, null, null)?.use { c ->
                val iL = c.getColumnIndexOrThrow("lseq")
                val iR = c.getColumnIndexOrThrow("record")
                while (c.moveToNext()) {
                    upto = maxOf(upto, c.getLong(iL))
                    records.put(JSONObject(c.getString(iR)))
                }
            } ?: return JSONObject().put("ok", false).put("error", "no access")
            if (records.length() > 0) {
                SelfkitSync.call(ctx, "mirror_apply", JSONObject().put("store", app).put("records", records).put("upto", upto))
                total += records.length()
            }
            since = upto
            if (records.length() < 500) break
        }
        return JSONObject().put("ok", true).put("records", total)
    }
}
