package com.selfkit.chrono

/** JNI bridge into the Rust library (src-tauri/src/lib.rs, module jni_bridge). */
object ChronoNative {
  init {
    System.loadLibrary("chrono_lib")
  }

  /** Next import cursor (UTC ms) for chrono.db in [dir], or -1. */
  @JvmStatic external fun cursor(dir: String): Long

  /** Import a Collector batch; returns JSON {ok, sessions, hours, alerts:[{id,title,body}]}. */
  @JvmStatic external fun ingest(dir: String, payload: String): String
}
