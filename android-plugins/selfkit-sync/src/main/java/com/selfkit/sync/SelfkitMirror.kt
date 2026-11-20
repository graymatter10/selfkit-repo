package com.selfkit.sync

import android.content.Context
import android.net.Uri
import android.util.Log
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
        val out = JSONObject().put("ok", true).put("records", total)
        for (t in TABLES[app].orEmpty()) {
            pullTable(ctx, authority, app, t)?.let { err ->
                // Records stay mirrored; the table copy failure is reported, not swallowed.
                Log.w("SelfkitMirror", "$app/$t: $err")
                out.put("table_error", "$t: $err")
            }
        }
        return out
    }

    /** Local tables copied whole next to the records (selfkit-appsync SHARED_TABLES). */
    private val TABLES = mapOf("dusk" to listOf("dusk_nights"))

    /** Rows per `/table` page: ~200 B per dusk_nights row keeps a page far below the ~2 MB CursorWindow. */
    private const val TABLE_PAGE = 1000

    /**
     * Copy one shared table page by page, then replace the mirror's copy in one go.
     * Returns null when done (or the source app is too old to share tables), else the error.
     */
    private fun pullTable(ctx: Context, authority: String, app: String, table: String): String? {
        return try {
            var sql: String? = null
            val all = JSONArray()
            var offset = 0L
            while (true) {
                val uri = Uri.parse("content://$authority/table?name=$table&offset=$offset&limit=$TABLE_PAGE")
                val c = ctx.contentResolver.query(uri, null, null, null, null) ?: return if (offset == 0L) null else "page at $offset unavailable"
                val (pageSql, rows, count) = c.use {
                    if (!it.moveToFirst()) return "empty reply"
                    val iC = it.getColumnIndex("count")
                    Triple(it.getString(it.getColumnIndexOrThrow("sql")), JSONArray(it.getString(it.getColumnIndexOrThrow("rows"))),
                        // An older provider has no count column and always sends the whole table.
                        if (iC < 0) -1L else it.getLong(iC))
                }
                sql = pageSql
                for (i in 0 until rows.length()) all.put(rows.get(i))
                if (count < TABLE_PAGE) break
                offset += count
            }
            val res = SelfkitSync.call(ctx, "mirror_table", JSONObject().put("store", app).put("name", table)
                .put("sql", sql).put("rows", all.toString()))
            if (res.optBoolean("ok")) null else res.optString("error", "copy failed")
        } catch (t: Throwable) {
            t.message ?: t.javaClass.simpleName
        }
    }
}
