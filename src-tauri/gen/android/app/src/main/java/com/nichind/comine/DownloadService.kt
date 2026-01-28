package com.nichind.comine

import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.media.MediaScannerConnection
import android.os.Binder
import android.os.IBinder
import android.util.Log
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

class DownloadService : Service() {
    companion object {
        private const val TAG = "DownloadService"

        const val EXTRA_JOB_ID = "jobId"
        const val EXTRA_REQUEST_JSON = "requestJson"

        const val ACTION_START = "com.nichind.comine.START"
        const val ACTION_PAUSE = "com.nichind.comine.PAUSE"
        const val ACTION_CANCEL = "com.nichind.comine.CANCEL"
        const val ACTION_PAUSE_ALL = "com.nichind.comine.PAUSE_ALL"
    }

    private val binder = LocalBinder()
    private lateinit var notificationManager: NotificationManager

    @Volatile private var foregroundStarted = false
    
    data class DownloadJob(
        var title: String,
        var progress: Int = 0,
        var stage: String = "Downloading",
        var outputPath: String? = null,
        var speedBps: Long? = null,
        var etaSeconds: Long? = null,
        var downloadedBytes: Long? = null,
        var totalBytes: Long? = null
    )
    
    private val activeJobs = ConcurrentHashMap<String, DownloadJob>()
    private val downloadExecutor = Executors.newFixedThreadPool(3)

    inner class LocalBinder : Binder() {
        fun getService(): DownloadService = this@DownloadService
    }

    override fun onCreate() {
        super.onCreate()
        notificationManager = getSystemService(NotificationManager::class.java)
        DownloadNotifications.init(this)
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

        // Start/update foreground summary notification.
        val summary = DownloadNotifications.buildSummaryNotification(activeJobs.keys)
        if (!foregroundStarted) {
            startForeground(DownloadNotifications.SUMMARY_NOTIFICATION_ID, summary)
            foregroundStarted = true
        } else {
            notificationManager.notify(DownloadNotifications.SUMMARY_NOTIFICATION_ID, summary)
        }

        DownloadNotifications.upsert(
            jobId = jobId,
            kind = DownloadNotifications.JobKind.DOWNLOAD,
            title = jobId,
            stage = "Starting",
            progress = 0,
            indeterminate = true,
            canPause = true,
            ongoing = true
        )

        downloadExecutor.execute {
            executeDownload(jobId, requestJson)
        }
    }
    
    private fun pauseJob(jobId: String) {
        val killed = YtDlp.cancel(jobId)
        if (killed) {
            RustBridge.notifyPaused(jobId)
        }
        removeJobAndUpdateSummary(jobId)
    }

    private fun cancelJob(jobId: String) {
        val killed = YtDlp.cancel(jobId)
        if (killed) {
            RustBridge.notifyCancelled(jobId)
        }
        removeJobAndUpdateSummary(jobId)
    }

    private fun removeJobAndUpdateSummary(jobId: String) {
        activeJobs.remove(jobId)
        DownloadNotifications.cancel(jobId)
        refreshSummaryNotification()
        maybeStopService()
    }

    private fun refreshSummaryNotification() {
        if (activeJobs.isEmpty()) {
            notificationManager.cancel(DownloadNotifications.SUMMARY_NOTIFICATION_ID)
        } else {
            notificationManager.notify(
                DownloadNotifications.SUMMARY_NOTIFICATION_ID,
                DownloadNotifications.buildSummaryNotification(activeJobs.keys)
            )
        }
    }

    private fun pauseAllJobs() {
        val ids = activeJobs.keys.toList()
        for (id in ids) {
            pauseJob(id)
        }
    }

    private fun updateStage(jobId: String, stage: String) {
        activeJobs[jobId]?.let { job ->
            job.stage = stage
        }

        val job = activeJobs[jobId]
        if (job != null) {
            DownloadNotifications.upsert(
                jobId = jobId,
                kind = DownloadNotifications.JobKind.DOWNLOAD,
                title = job.title,
                stage = stage,
                progress = job.progress,
                indeterminate = stage.lowercase().contains("embed") || stage.lowercase().contains("post"),
                speedBps = job.speedBps,
                etaSeconds = job.etaSeconds,
                downloadedBytes = job.downloadedBytes,
                totalBytes = job.totalBytes,
                canPause = true,
                ongoing = true
            )
        }

        refreshSummaryNotification()
    }

    private fun updateTitle(jobId: String, title: String) {
        val t = title.trim()
        if (t.isBlank()) return

        val job = activeJobs[jobId] ?: return
        if (job.title == t) return

        job.title = t
        DownloadNotifications.upsert(
            jobId = jobId,
            kind = DownloadNotifications.JobKind.DOWNLOAD,
            title = job.title,
            stage = job.stage,
            progress = job.progress,
            indeterminate = false,
            speedBps = job.speedBps,
            etaSeconds = job.etaSeconds,
            downloadedBytes = job.downloadedBytes,
            totalBytes = job.totalBytes,
            canPause = true,
            ongoing = true
        )

        refreshSummaryNotification()
    }

    fun updateProgress(
        jobId: String,
        progress: Int,
        speedBps: Long? = null,
        etaSeconds: Long? = null,
        downloadedBytes: Long? = null,
        totalBytes: Long? = null,
    ) {
        activeJobs[jobId]?.let { job ->
            job.progress = progress.coerceIn(0, 100)
            job.speedBps = speedBps
            job.etaSeconds = etaSeconds
            job.downloadedBytes = downloadedBytes
            job.totalBytes = totalBytes
        }

        val job = activeJobs[jobId]
        if (job != null) {
            DownloadNotifications.upsert(
                jobId = jobId,
                kind = DownloadNotifications.JobKind.DOWNLOAD,
                title = job.title,
                stage = job.stage,
                progress = job.progress,
                indeterminate = false,
                speedBps = job.speedBps,
                etaSeconds = job.etaSeconds,
                downloadedBytes = job.downloadedBytes,
                totalBytes = job.totalBytes,
                canPause = true,
                ongoing = true
            )
        }

        refreshSummaryNotification()
    }

    override fun onBind(intent: Intent?): IBinder = binder

    private fun maybeStopService() {
        if (activeJobs.isEmpty()) {
            try {
                stopForeground(STOP_FOREGROUND_REMOVE)
            } catch (_: Exception) {
            }
            foregroundStarted = false
            stopSelf()
        }
    }

    private fun executeDownload(jobId: String, requestJson: String) {
        if (!YtDlp.initialized) {
            Log.e(TAG, "executeDownload: yt-dlp not initialized for job $jobId")
            val err = "yt-dlp not initialized. Please wait for the app to fully start."
            RustBridge.notifyFailed(jobId, err)
            activeJobs.remove(jobId)

            DownloadNotifications.fail(jobId, title = jobId, error = err)
            refreshSummaryNotification()
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

        activeJobs[jobId]?.let { it.stage = "Downloading"; it.progress = 0 }
        activeJobs[jobId] = activeJobs[jobId]?.copy(title = initialTitle) ?: DownloadJob(title = initialTitle)
        DownloadNotifications.upsert(
            jobId = jobId,
            kind = DownloadNotifications.JobKind.DOWNLOAD,
            title = initialTitle,
            stage = "Downloading",
            progress = 0,
            indeterminate = false,
            canPause = true,
            ongoing = true
        )

        val result = try {
            YtDlp.execute(
                jobId = jobId,
                requestJson = requestJson,
                onProgress = { u ->
                    updateProgress(
                        jobId = jobId,
                        progress = u.percent,
                        speedBps = u.speedBps,
                        etaSeconds = u.etaSeconds,
                        downloadedBytes = u.downloadedBytes,
                        totalBytes = u.totalBytes
                    )
                },
                onStage = { stage ->
                    updateStage(jobId, stage)
                },
                onTitle = { title ->
                    updateTitle(jobId, title)
                }
            )
        } catch (e: Exception) {
            Log.e(TAG, "executeDownload exception for job $jobId", e)
            YtDlp.ExecuteResult.Failed(e.message ?: "Unknown error during download")
        }

        when (result) {
            is YtDlp.ExecuteResult.Success -> {
                updateProgress(jobId, 100)
                activeJobs[jobId]?.outputPath = result.outputPath
                DownloadNotifications.complete(
                    jobId = jobId,
                    title = result.title ?: initialTitle,
                    info = "Saved",
                    outputPath = result.outputPath,
                    thumbnailUrl = result.thumbnailUrl
                )
                RustBridge.notifyCompleted(jobId, result.outputPath, result.title, null)
                
                // Scan the file so it appears in gallery immediately
                if (result.outputPath != null) {
                    MediaScannerConnection.scanFile(this, arrayOf(result.outputPath), null, null)
                }
            }
            is YtDlp.ExecuteResult.Failed -> {
                DownloadNotifications.fail(jobId, title = initialTitle, error = result.error)
                RustBridge.notifyFailed(jobId, result.error)
            }
        }

        activeJobs.remove(jobId)
        refreshSummaryNotification()
        maybeStopService()
    }
}
