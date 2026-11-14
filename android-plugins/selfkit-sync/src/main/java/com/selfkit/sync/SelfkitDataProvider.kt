package com.selfkit.sync

import android.content.ContentProvider
import android.content.ContentValues
import android.content.pm.PackageManager
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Bundle
import org.json.JSONObject

/**
 * Companion-ready data access, `content://com.selfkit.<app>.provider/...`, guarded by the
 * signature permission com.selfkit.permission.SHARE (only Selfkit apps signed with the
 * same key). Goes through selfkit-appsync/selfkit-core so history and dirty flags stay right.
 *
 *  query  /changes?store=<app|settings>&since=<lseq>&limit=500  -> lseq, id, collection, data, updated_at, deleted
 *  insert /records  values: store?, collection, id?, data(JSON)  -> /records/<id>
 *  call   "status" | "sync_now" | "settings" | "set_settings" | "schedule" | "conflicts"   (arg/extras "json")
 *         -> Bundle { "json": <result> }
 */
class SelfkitDataProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    private fun guard() {
        val ctx = context ?: throw SecurityException("no context")
        if (ctx.checkCallingOrSelfPermission(PERMISSION) != PackageManager.PERMISSION_GRANTED) {
            throw SecurityException("$PERMISSION required")
        }
    }

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor? {
        guard()
        if (uri.pathSegments.firstOrNull() != "changes") return null
        val ctx = context!!
        val args = JSONObject()
            .put("store", uri.getQueryParameter("store") ?: Meta.app(ctx))
            .put("since", uri.getQueryParameter("since")?.toLongOrNull() ?: 0L)
            .put("limit", uri.getQueryParameter("limit")?.toLongOrNull() ?: 500L)
        val res = SelfkitSync.call(ctx, "changes", args)
        val c = MatrixCursor(arrayOf("lseq", "id", "collection", "data", "updated_at", "deleted"))
        val rows = res.optJSONArray("rows") ?: return c
        for (i in 0 until rows.length()) {
            val row = rows.getJSONObject(i)
            val r = row.getJSONObject("record")
            c.addRow(arrayOf<Any?>(row.getLong("lseq"), r.getString("id"), r.getString("collection"), r.get("data").toString(), r.getLong("updated_at"), if (r.getBoolean("deleted")) 1 else 0))
        }
        return c
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? {
        guard()
        val ctx = context!!
        val v = values ?: return null
        val args = JSONObject()
            .put("store", v.getAsString("store") ?: Meta.app(ctx))
            .put("collection", v.getAsString("collection"))
            .put("data", JSONObject(v.getAsString("data")))
        v.getAsString("id")?.let { args.put("id", it) }
        val r = SelfkitSync.call(ctx, "put", args)
        return Uri.withAppendedPath(uri, r.optString("id"))
    }

    override fun call(method: String, arg: String?, extras: Bundle?): Bundle {
        guard()
        val ctx = context!!
        val allowed = setOf("status", "sync_now", "settings", "set_settings", "schedule", "conflicts", "resolve")
        if (method !in allowed) throw IllegalArgumentException("unknown method $method")
        val json = extras?.getString("json") ?: arg ?: "{}"
        val out = if (method == "sync_now") SelfkitSync.syncBlocking(ctx, JSONObject(json).optString("trigger", "manual"))
                  else SelfkitSync.call(ctx, method, JSONObject(json))
        if (method == "set_settings") SelfkitSync.schedule(ctx)
        return Bundle().apply { putString("json", out.toString()) }
    }

    override fun getType(uri: Uri): String = "vnd.android.cursor.dir/vnd.selfkit.record"
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    companion object {
        const val PERMISSION = "com.selfkit.permission.SHARE"
    }
}
