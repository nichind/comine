package com.nichind.comine

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

class DownloadService : Service() {
    companion object {
        private const val TAG = "DownloadService"

        const val CHANNEL_ID = "comine_downloads"
        const val NOTIFICATION_ID = 1

        const val EXTRA_JOB_ID = "jobId"
        const val EXTRA_REQUEST_JSON = "requestJson"

        const val ACTION_START = "com.nichind.comine.START"
        const val ACTION_PAUSE = "com.nichind.comine.PAUSE"
        const val ACTION_CANCEL = "com.nichind.comine.CANCEL"
        const val ACTION_PAUSE_ALL = "com.nichind.comine.PAUSE_ALL"
    }

    private val binder = LocalBinder()
    private lateinit var notificationManager: NotificationManager
    
    data class DownloadJob(
        val title: String,
        var progress: Int = 0,
        var speed: String = "",
        var eta: String = "",
        var outputPath: String? = null
    )
    
    private val activeJobs = ConcurrentHashMap<String, DownloadJob>()
    private val downloadExecutor = Executors.newFixedThreadPool(3)

    inner class LocalBinder : Binder() {
        fun getService(): DownloadService = this@DownloadService
    }

    override fun onCreate() {
        super.onCreate()
        notificationManager = getSystemService(NotificationManager::class.java)
        createNotificationChannel()
        YtDlp.init(application)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> {
                val jobId = intent.getStringExtra(EXTRA_JOB_ID)
                val requestJson = intent.getStringExtra(EXTRA_REQUEST_JSON)
                if (!jobId.isNullOrBlank() && !requestJson.isNullOrBlank()) {
                    startDownload(jobId, requestJson)
                }
            }
            ACTION_PAUSE -> {
                val jobId = intent.getStringExtra(EXTRA_JOB_ID)
                jobId?.let { pauseJob(it) }
            }
            ACTION_CANCEL -> {
                val jobId = intent.getStringExtra(EXTRA_JOB_ID)
                if (jobId == "__ALL__") {
                    val ids = activeJobs.keys.toList()
                    for (id in ids) {
                        cancelJob(id)
                    }
                } else {
                    jobId?.let { cancelJob(it) }
                }
            }
            ACTION_PAUSE_ALL -> pauseAllJobs()
        }
        return START_STICKY
    }

    fun startDownload(jobId: String, requestJson: String) {
        activeJobs[jobId] = DownloadJob(title = jobId)
        startForeground(NOTIFICATION_ID, buildNotification())

        downloadExecutor.execute {
            executeDownload(jobId, requestJson)
        }
    }
    
    private fun pauseJob(jobId: String) {
        val killed = YtDlp.cancel(jobId)
        if (killed) {
            RustBridge.notifyPaused(jobId)
        }
        activeJobs.remove(jobId)
        notificationManager.notify(NOTIFICATION_ID, buildNotification())
        maybeStopService()
    }

    private fun cancelJob(jobId: String) {
        val killed = YtDlp.cancel(jobId)
        if (killed) {
            RustBridge.notifyCancelled(jobId)
        }
        activeJobs.remove(jobId)
        notificationManager.notify(NOTIFICATION_ID, buildNotification())
        maybeStopService()
    }

    private fun pauseAllJobs() {
        val ids = activeJobs.keys.toList()
        for (id in ids) {
            pauseJob(id)
        }
    }

    private fun buildNotification(): Notification {
        val totalProgress = calculateTotalProgress()
        val activeCount = activeJobs.size

        val pauseAllIntent = PendingIntent.getService(
            this, 0,
            Intent(this, DownloadService::class.java).apply { action = ACTION_PAUSE_ALL },
            PendingIntent.FLAG_IMMUTABLE
        )

        val cancelAllIntent = PendingIntent.getService(
            this, 1,
            Intent(this, DownloadService::class.java).apply {
                action = ACTION_CANCEL
                putExtra(EXTRA_JOB_ID, "__ALL__")
            },
            PendingIntent.FLAG_IMMUTABLE
        )
        
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Downloading $activeCount file(s)")
            .setContentText("$totalProgress% complete")
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setProgress(100, totalProgress, false)
            .setOngoing(true)
            .addAction(android.R.drawable.ic_media_pause, "Pause All", pauseAllIntent)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "Cancel All", cancelAllIntent)
            .build()
    }

    private fun calculateTotalProgress(): Int {
        if (activeJobs.isEmpty()) return 0
        val sum = activeJobs.values.sumOf { it.progress }
        return sum / activeJobs.size
    }

    fun updateProgress(jobId: String, progress: Int, speed: String, eta: String) {
        activeJobs[jobId]?.let { job ->
            job.progress = progress
            job.speed = speed
            job.eta = eta
        }

        notificationManager.notify(NOTIFICATION_ID, buildNotification())
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Downloads",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Shows download progress"
                setShowBadge(false)
            }
            notificationManager.createNotificationChannel(channel)
        }
    }

    override fun onBind(intent: Intent?): IBinder = binder

    private fun maybeStopService() {
        if (activeJobs.isEmpty()) {
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
                    stopForeground(STOP_FOREGROUND_REMOVE)
                } else {
                    @Suppress("DEPRECATION")
                    stopForeground(true)
                }
            } catch (_: Exception) {
            }
            stopSelf()
        }
    }

    private fun executeDownload(jobId: String, requestJson: String) {
        if (!YtDlp.initialized) {
            Log.e(TAG, "executeDownload: yt-dlp not initialized for job $jobId")
            RustBridge.notifyFailed(jobId, "yt-dlp not initialized. Please wait for the app to fully start.")
            activeJobs.remove(jobId)
            notificationManager.notify(NOTIFICATION_ID, buildNotification())
            maybeStopService()
            return
        }
        
        val initialTitle = try {
            val urlObj = java.net.URL(requestJson.let { 
                org.json.JSONObject(it).optString("url", "") 
            })
            urlObj.path.substringAfterLast("/").takeIf { it.isNotBlank() } ?: "Downloading..."
        } catch (_: Exception) {
            "Downloading..."
        }
        
        RustBridge.notifyStarted(jobId, initialTitle)

        val result = try {
            YtDlp.execute(jobId, requestJson) { progress, eta ->
                updateProgress(jobId, progress, "", eta)
            }
        } catch (e: Exception) {
            Log.e(TAG, "executeDownload exception for job $jobId", e)
            YtDlp.ExecuteResult.Failed(e.message ?: "Unknown error during download")
        }

        when (result) {
            is YtDlp.ExecuteResult.Success -> {
                updateProgress(jobId, 100, "", "")
                activeJobs[jobId]?.outputPath = result.outputPath
                RustBridge.notifyCompleted(jobId, result.outputPath, result.title, null)
            }
            is YtDlp.ExecuteResult.Failed -> {
                RustBridge.notifyFailed(jobId, result.error)
            }
        }

        activeJobs.remove(jobId)
        notificationManager.notify(NOTIFICATION_ID, buildNotification())
        maybeStopService()
    }
}
