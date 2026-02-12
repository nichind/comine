package com.nichind.comine

import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.IBinder
import android.util.Log
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

class DownloadService : Service() {
    companion object {
        private const val TAG = "DownloadService"

        const val EXTRA_JOB_ID = "jobId"
        const val EXTRA_PAYLOAD = "payload"
        const val EXTRA_BACKEND = "backend"
        const val EXTRA_TITLE = "title"

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
        var backendName: String,
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
                val payload = intent.getStringExtra(EXTRA_PAYLOAD)
                val backendName = intent.getStringExtra(EXTRA_BACKEND)
                val title = intent.getStringExtra(EXTRA_TITLE) ?: ""
                
                if (!jobId.isNullOrBlank() && !payload.isNullOrBlank() && !backendName.isNullOrBlank()) {
                    startJob(jobId, backendName, payload, title)
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

    fun startJob(jobId: String, backendName: String, payloadJson: String, title: String) {
        val displayTitle = if (title.isNotBlank()) title else jobId
        activeJobs[jobId] = DownloadJob(title = displayTitle, backendName = backendName)

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
            title = displayTitle,
            stage = "Starting",
            progress = 0,
            indeterminate = true,
            canPause = true,
            ongoing = true
        )

        downloadExecutor.execute {
            executeJob(jobId, backendName, payloadJson, title)
        }
    }
    
    private fun pauseJob(jobId: String) {
        val job = activeJobs[jobId]
        if (job != null) {
            val backend = BackendRegistry.get(job.backendName)
            
            backend?.cancel(jobId)
            RustBridge.notifyPaused(jobId)
            removeJobAndUpdateSummary(jobId)
        }
    }

    private fun cancelJob(jobId: String) {
        val job = activeJobs[jobId]
        if (job != null) {
            val backend = BackendRegistry.get(job.backendName)
            backend?.cancel(jobId)
            RustBridge.notifyCancelled(jobId)
            removeJobAndUpdateSummary(jobId)
        }
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

    private fun executeJob(jobId: String, backendName: String, payloadJson: String, title: String) {
        val backend = BackendRegistry.get(backendName)
        if (backend == null) {
            val err = "Backend '$backendName' not found"
            Log.e(TAG, err)
            RustBridge.notifyFailed(jobId, err)
            removeJobAndUpdateSummary(jobId)
            return
        }

        val payload = try {
             org.json.JSONObject(payloadJson)
        } catch (e: Exception) {
             val err = "Invalid payload JSON: ${e.message}"
             RustBridge.notifyFailed(jobId, err)
             removeJobAndUpdateSummary(jobId)
             return
        }
        
        val initialTitle = if (title.isNotBlank()) title else jobId
        RustBridge.notifyStarted(jobId, initialTitle)
        
        updateStage(jobId, "Running")

        val result = try {
            backend.execute(applicationContext, jobId, payload) { progress, speed, eta ->
                // speed string from backend might need parsing if we want valid speedBps
                updateProgress(jobId, progress, etaSeconds = eta)
            }
        } catch (e: Exception) {
            Log.e(TAG, "Job execution failed", e)
            JobResult.Failed(e.message ?: "Unknown error")
        }

        when (result) {
            is JobResult.Success -> {
                updateProgress(jobId, 100)
                activeJobs[jobId]?.outputPath = result.outputPath
                DownloadNotifications.complete(
                    jobId = jobId,
                    title = result.title ?: initialTitle,
                    info = "Saved",
                    outputPath = result.outputPath,
                    thumbnailUrl = result.thumbnail
                )
                
                backend.notifyRustSuccess(jobId, result)
            }
            is JobResult.Failed -> {
                DownloadNotifications.fail(jobId, title = initialTitle, error = result.error)
                backend.notifyRustFailure(jobId, result.error)
            }
            is JobResult.Cancelled -> {
                RustBridge.notifyCancelled(jobId) // Generic enough usually
            }
        }

        removeJobAndUpdateSummary(jobId)
    }
}
