package com.selfkit.sync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Runs one sync pass (periodic ~15 min job, or the one-off "on open" run). */
class SyncWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val trigger = inputData.getString(KEY_TRIGGER) ?: "background"
        try {
            SelfkitSync.syncBlocking(applicationContext, trigger)
            Result.success() // failures are in the status; the next period retries
        } catch (t: Throwable) {
            Result.success()
        }
    }

    companion object {
        const val KEY_TRIGGER = "trigger"
    }
}
