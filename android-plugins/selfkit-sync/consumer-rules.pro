# JNI: native code looks these up by name.
-keep class com.selfkit.sync.SelfkitSyncNative { *; }
-keep class com.selfkit.sync.SyncWorker { *; }
-keep class com.selfkit.sync.SelfkitDataProvider { *; }
# WorkManager (used by SyncWorker): R8 otherwise strips its Room database constructor
# and the app crashes at start (androidx.startup InitializationProvider).
-keep class androidx.work.impl.** { *; }
-keep class * extends androidx.room.RoomDatabase { <init>(); }
-keep class * extends androidx.work.ListenableWorker { <init>(android.content.Context, androidx.work.WorkerParameters); }
