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

    /**
     * Keep the shared settings (main currency, day start, theme) the same in every Selfkit
     * app on this phone: read each sibling app's settings record through its provider and
     * merge it field by field (newest change per field wins). Cheap: one record per app.
     */
    fun pullSettings(ctx: Context) {
        val me = Meta.app(ctx)
        for (app in listOf("chrono", "hisab", "agenda", "dusk", "reflect")) {
            if (app == me) continue
            try {
                val authority = "com.selfkit.$app.provider"
                if (ctx.packageManager.resolveContentProvider(authority, 0) == null) continue
                val records = JSONArray()
                ctx.contentResolver.query(Uri.parse("content://$authority/changes?store=settings&since=0&limit=50"), null, null, null, null)?.use { c ->
                    val iR = c.getColumnIndexOrThrow("record")
                    while (c.moveToNext()) records.put(JSONObject(c.getString(iR)))
                }
                if (records.length() > 0) SelfkitSync.call(ctx, "settings_adopt", JSONObject().put("records", records))
            } catch (t: Throwable) { /* app without the plugin, or not signed by the same key */ }
        }
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
        for (t in TABLES[app].orEmpty()) pullTable(ctx, authority, app, t)
        return JSONObject().put("ok", true).put("records", total)
    }

    /** Local tables copied whole next to the records (selfkit-appsync SHARED_TABLES). */
    private val TABLES = mapOf("dusk" to listOf("dusk_nights"))

    /** Copy one shared table; an older app without `/table` (null cursor) is skipped. */
    private fun pullTable(ctx: Context, authority: String, app: String, table: String) {
        try {
            ctx.contentResolver.query(Uri.parse("content://$authority/table?name=$table"), null, null, null, null)?.use { c ->
                if (!c.moveToFirst()) return
                SelfkitSync.call(ctx, "mirror_table", JSONObject().put("store", app).put("name", table)
                    .put("sql", c.getString(c.getColumnIndexOrThrow("sql"))).put("rows", c.getString(c.getColumnIndexOrThrow("rows"))))
            }
        } catch (t: Throwable) { /* keep the records mirror even if the table copy fails */ }
    }
}
