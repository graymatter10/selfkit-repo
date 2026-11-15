package com.selfkit.sync

import android.content.Context
import android.content.pm.PackageManager

/**
 * JNI bridge to selfkit-appsync (Rust, feature "android"). The symbols live in the
 * app's own Rust library, named by the manifest meta-data `com.selfkit.sync.LIB`
 * (e.g. "agenda_lib"). Every call takes and returns JSON.
 */
object SelfkitSyncNative {
    @Volatile private var loaded = false

    external fun call(app: String, dataDir: String, wrapKey: ByteArray?, deviceName: String, method: String, argsJson: String): String
    external fun setWrapKey(key: ByteArray)

    /** Loads the app's Rust library if needed (a WorkManager job may run before any Activity). */
    @Synchronized
    fun ensureLoaded(ctx: Context) {
        if (loaded) return
        System.loadLibrary(Meta.lib(ctx))
        loaded = true
    }
}

internal object Meta {
    private fun meta(ctx: Context): android.os.Bundle {
        val pm = ctx.packageManager
        val info = if (android.os.Build.VERSION.SDK_INT >= 33) {
            pm.getApplicationInfo(ctx.packageName, PackageManager.ApplicationInfoFlags.of(PackageManager.GET_META_DATA.toLong()))
        } else {
            @Suppress("DEPRECATION") pm.getApplicationInfo(ctx.packageName, PackageManager.GET_META_DATA)
        }
        return info.metaData ?: android.os.Bundle()
    }

    /** "agenda", "hisab", ... (meta-data com.selfkit.sync.APP, default: last part of the package name). */
    fun app(ctx: Context): String = meta(ctx).getString("com.selfkit.sync.APP") ?: ctx.packageName.substringAfterLast('.')

    /** Rust library name without "lib"/".so" (meta-data com.selfkit.sync.LIB, default "<app>_lib"). */
    fun lib(ctx: Context): String = meta(ctx).getString("com.selfkit.sync.LIB") ?: "${app(ctx)}_lib"

    /** Same folder the app's Rust code uses as the Selfkit data dir (Tauri: app data dir). */
    /** Apps whose data this app mirrors read-only (meta-data com.selfkit.sync.MIRROR, e.g. "chrono,hisab"). */
    fun mirror(ctx: Context): List<String> = (meta(ctx).getString("com.selfkit.sync.MIRROR") ?: "").split(',').map { it.trim() }.filter { it.isNotEmpty() }

    /** Tauri's app_data_dir on Android is the app's dataDir (/data/user/0/<package>). */
    fun dataDir(ctx: Context): String = meta(ctx).getString("com.selfkit.sync.DATA_SUBDIR")
        ?.let { java.io.File(ctx.dataDir, it).absolutePath }
        ?: ctx.dataDir.absolutePath
}
