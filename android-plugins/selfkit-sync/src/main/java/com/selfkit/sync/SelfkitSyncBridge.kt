package com.selfkit.sync

import android.app.Activity
import android.webkit.JavascriptInterface

/**
 * JS bridge for the shared Sync screen (ui/src/sync): `window.SelfkitSync`.
 * Register in MainActivity.onWebViewCreate:
 *   webView.addJavascriptInterface(SelfkitSyncBridge(this), "SelfkitSync")
 */
class SelfkitSyncBridge(private val activity: Activity) {
    @JavascriptInterface
    fun needsBatteryTip(): Boolean = try { SelfkitSync.needsBatteryTip(activity) } catch (t: Throwable) { false }

    @JavascriptInterface
    fun openBatterySettings() {
        activity.runOnUiThread { SelfkitSync.openBatterySettings(activity) }
    }

    /** Re-apply the periodic job after the sync settings changed (owner, automatic). */
    @JavascriptInterface
    fun schedule() {
        Thread { try { SelfkitSync.schedule(activity) } catch (t: Throwable) { } }.start()
    }

    /** Refresh read-only copies of other Selfkit apps' data (comma-separated app names). Blocking; returns JSON. */
    @JavascriptInterface
    fun mirror(apps: String): String = try {
        SelfkitMirror.pull(activity, apps.split(',').map { it.trim() }.filter { it.isNotEmpty() }).toString()
    } catch (t: Throwable) { "{}" }

    /** Background pass without waiting (the Rust "Sync now" command waits instead). */
    @JavascriptInterface
    fun runNow() = SelfkitSync.runOnce(activity, "manual")
}
