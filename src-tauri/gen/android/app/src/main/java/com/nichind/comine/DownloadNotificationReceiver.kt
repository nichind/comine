package com.nichind.comine

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

class DownloadNotificationReceiver : BroadcastReceiver() {
    companion object {
        private const val TAG = "DlNotifReceiver"

        const val ACTION_CANCEL = "com.nichind.comine.NOTIF_CANCEL"
        const val ACTION_PAUSE = "com.nichind.comine.NOTIF_PAUSE"

        const val EXTRA_JOB_ID = "jobId"
        const val EXTRA_KIND = "kind" // DownloadNotifications.JobKind name
    }

    override fun onReceive(context: Context, intent: Intent) {
        val jobId = intent.getStringExtra(EXTRA_JOB_ID)
        val kind = intent.getStringExtra(EXTRA_KIND)
        if (jobId.isNullOrBlank()) return

        runCatching { DownloadNotifications.init(context) }

        when (intent.action) {
            ACTION_CANCEL -> {
                Log.i(TAG, "Cancel from notification: jobId=$jobId kind=$kind")

                // Try to cancel all possible job runners. Only one should actually be active.
                runCatching { YtDlp.cancel(jobId) }
                runCatching { Aria2.cancel(jobId) }
                runCatching { FFmpegExecutor.cancel(jobId) }
                runCatching { FFmpegExecutor.cancel("$jobId:thumb") }
                runCatching { FFmpegExecutor.cancel("$jobId:thumbimg") }

                // Also tell DownloadService (if running) so it can remove the job from its active list.
                runCatching {
                    context.startService(Intent(context, DownloadService::class.java).apply {
                        action = DownloadService.ACTION_CANCEL
                        putExtra(DownloadService.EXTRA_JOB_ID, jobId)
                    })
                }

                // Notify Rust so frontend queue matches user intent.
                if (kind == DownloadNotifications.JobKind.CONVERT.name) {
                    runCatching { RustBridge.notifyConvertFailed(jobId, "Cancelled") }
                } else {
                    runCatching { RustBridge.notifyCancelled(jobId) }
                }

                DownloadNotifications.cancel(jobId)
            }
            ACTION_PAUSE -> {
                Log.i(TAG, "Pause from notification: jobId=$jobId")
                runCatching {
                    context.startService(Intent(context, DownloadService::class.java).apply {
                        action = DownloadService.ACTION_PAUSE
                        putExtra(DownloadService.EXTRA_JOB_ID, jobId)
                    })
                }
            }
        }
    }
}
