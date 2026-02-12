package com.nichind.comine

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.FileProvider
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import kotlin.math.abs

object DownloadNotifications {
    private const val TAG = "DownloadNotifications"
    const val CHANNEL_ID = "comine_downloads"
    private const val CHANNEL_NAME = "Downloads"
    private const val GROUP_KEY = "comine_downloads_group"
    
    private val mainHandler = Handler(Looper.getMainLooper())

    // Foreground-summary notification ID (used by DownloadService)
    const val SUMMARY_NOTIFICATION_ID = 1

    enum class JobKind { DOWNLOAD, CONVERT }

    data class JobState(
        val jobId: String,
        val kind: JobKind,
        var title: String,
        var stage: String = "Downloading",
        var progress: Int? = null,
        var indeterminate: Boolean = false,
        var speedBps: Long? = null,
        var etaSeconds: Long? = null,
        var downloadedBytes: Long? = null,
        var totalBytes: Long? = null,
    )

    @Volatile
    private var initialized = false
    private var appContext: Context? = null
    private var notificationManager: NotificationManager? = null

    private val jobs = ConcurrentHashMap<String, JobState>()
    private val completedJobs = ConcurrentHashMap.newKeySet<String>()
    private val thumbExecutor = Executors.newSingleThreadExecutor()

    fun init(context: Context) {
        if (initialized) return
        appContext = context.applicationContext
        notificationManager = appContext!!.getSystemService(NotificationManager::class.java)
        ensureChannel()
        initialized = true
    }

    private fun ensureInitOrReturn(): Pair<Context, NotificationManager>? {
        if (!initialized) return null
        val ctx = appContext ?: return null
        val nm = notificationManager ?: return null
        return ctx to nm
    }

    fun upsert(
        jobId: String,
        kind: JobKind,
        title: String? = null,
        stage: String? = null,
        progress: Int? = null,
        indeterminate: Boolean? = null,
        speedBps: Long? = null,
        etaSeconds: Long? = null,
        downloadedBytes: Long? = null,
        totalBytes: Long? = null,
        canPause: Boolean = false,
        ongoing: Boolean = true,
    ) {
        if (completedJobs.contains(jobId)) return
        val (ctx, nm) = ensureInitOrReturn() ?: return

        val state = jobs[jobId] ?: JobState(jobId = jobId, kind = kind, title = title ?: jobId)
        title?.let { state.title = it }
        stage?.let { state.stage = it }
        progress?.let { state.progress = it.coerceIn(0, 100) }
        indeterminate?.let { state.indeterminate = it }
        speedBps?.let { if (it > 0) state.speedBps = it }
        etaSeconds?.let { if (it >= 0) state.etaSeconds = it }
        downloadedBytes?.let { if (it >= 0) state.downloadedBytes = it }
        totalBytes?.let { if (it > 0) state.totalBytes = it }
        jobs[jobId] = state

        // Double-check before notifying to prevent race with complete()
        if (completedJobs.contains(jobId)) return
        nm.notify(jobIdToNotificationId(jobId), buildJobNotification(ctx, state, canPause, ongoing))
    }

    fun complete(
        jobId: String,
        title: String? = null,
        info: String? = null,
        outputPath: String? = null,
        thumbnailUrl: String? = null,
        kind: JobKind = JobKind.DOWNLOAD,
    ) {
        Log.i(TAG, "complete() called for job $jobId, kind=$kind")
        completedJobs.add(jobId)
        val (ctx, nm) = ensureInitOrReturn() ?: run {
            Log.e(TAG, "complete() early return - not initialized")
            return
        }

        val existing = jobs.remove(jobId)
        val displayTitle = title ?: existing?.title ?: jobId
        val actualKind = existing?.kind ?: kind
        
        val fileInfo = buildString {
            if (outputPath != null) {
                val file = File(outputPath)
                if (file.exists()) {
                    append(formatFileSize(file.length()))
                    append(" • ")
                }
            }
            append(info ?: "Saved")
        }

        val notifId = jobIdToNotificationId(jobId)
        Log.i(TAG, "Showing completed notification: id=$notifId, title=$displayTitle, kind=$actualKind")
        
        mainHandler.post {
            val builder = buildCompletedNotification(ctx, displayTitle, fileInfo, outputPath, null, actualKind)
            nm.notify(notifId, builder.build())
            updateSummaryNotification()
        }

        if (!thumbnailUrl.isNullOrBlank()) {
            thumbExecutor.execute {
                val thumb = loadThumbnailBitmap(thumbnailUrl)
                if (thumb != null) {
                    mainHandler.post {
                        val updatedBuilder = buildCompletedNotification(ctx, displayTitle, fileInfo, outputPath, thumb, actualKind)
                        nm.notify(notifId, updatedBuilder.build())
                    }
                }
            }
        }
        
        // Clean up completedJobs after a delay to prevent race conditions
        mainHandler.postDelayed({ completedJobs.remove(jobId) }, 5000)
    }

    private fun buildCompletedNotification(
        ctx: Context,
        title: String,
        info: String,
        outputPath: String?,
        thumbnail: Bitmap?,
        kind: JobKind = JobKind.DOWNLOAD,
    ): NotificationCompat.Builder {
        val icon = if (kind == JobKind.CONVERT) android.R.drawable.stat_notify_sync else android.R.drawable.stat_sys_download_done
        
        val builder = NotificationCompat.Builder(ctx, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(info)
            .setSmallIcon(icon)
            .setGroup(GROUP_KEY)
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)

        if (thumbnail != null) {
            builder.setLargeIcon(thumbnail)
            builder.setStyle(
                NotificationCompat.BigPictureStyle()
                    .bigPicture(thumbnail)
                    .bigLargeIcon(null as Bitmap?)  // Hide large icon when expanded (thumbnail takes over)
            )
        }

        if (outputPath != null) {
            val openIntent = createOpenFileIntent(ctx, outputPath)
            if (openIntent != null) {
                val pendingOpen = PendingIntent.getActivity(
                    ctx,
                    jobIdToNotificationId(outputPath) + 20,
                    openIntent,
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
                )
                builder.setContentIntent(pendingOpen)

                val shareIntent = createShareIntent(ctx, outputPath)
                if (shareIntent != null) {
                    val pendingShare = PendingIntent.getActivity(
                        ctx,
                        jobIdToNotificationId(outputPath) + 21,
                        Intent.createChooser(shareIntent, "Share"),
                        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
                    )
                    builder.addAction(android.R.drawable.ic_menu_share, "Share", pendingShare)
                }
            }
        }

        return builder
    }

    private fun createOpenFileIntent(ctx: Context, filePath: String): Intent? {
        return try {
            val file = File(filePath)
            if (!file.exists()) return null
            
            val uri = FileProvider.getUriForFile(ctx, "${ctx.packageName}.fileprovider", file)
            val mime = ctx.contentResolver.getType(uri) ?: getMimeType(filePath)
            
            Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mime)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            }
        } catch (_: Exception) {
            null
        }
    }

    private fun createShareIntent(ctx: Context, filePath: String): Intent? {
        return try {
            val file = File(filePath)
            if (!file.exists()) return null
            
            val uri = FileProvider.getUriForFile(ctx, "${ctx.packageName}.fileprovider", file)
            val mime = ctx.contentResolver.getType(uri) ?: getMimeType(filePath)
            
            Intent(Intent.ACTION_SEND).apply {
                type = mime
                putExtra(Intent.EXTRA_STREAM, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
        } catch (_: Exception) {
            null
        }
    }

    private fun getMimeType(filePath: String): String {
        val ext = filePath.substringAfterLast('.', "").lowercase()
        return when (ext) {
            "mp4", "m4v", "mkv", "webm", "avi", "mov" -> "video/*"
            "mp3", "m4a", "ogg", "opus", "flac", "wav", "aac" -> "audio/*"
            "jpg", "jpeg", "png", "webp", "gif" -> "image/*"
            else -> "*/*"
        }
    }

    private fun loadThumbnailBitmap(url: String): Bitmap? {
        return try {
            val connection = java.net.URL(url).openConnection()
            connection.connectTimeout = 5000
            connection.readTimeout = 5000
            connection.getInputStream().use { input ->
                val bitmap = BitmapFactory.decodeStream(input) ?: return null
                val size = 128
                val scaled = Bitmap.createScaledBitmap(bitmap, size, size, true)
                if (scaled !== bitmap) bitmap.recycle()
                scaled
            }
        } catch (_: Exception) {
            null
        }
    }

    private fun formatFileSize(bytes: Long): String {
        val kb = 1024.0
        val mb = kb * 1024.0
        val gb = mb * 1024.0
        val v = bytes.toDouble()
        return when {
            v >= gb -> String.format("%.2f GB", v / gb)
            v >= mb -> String.format("%.1f MB", v / mb)
            v >= kb -> String.format("%.0f KB", v / kb)
            else -> "$bytes B"
        }
    }

    fun fail(jobId: String, title: String? = null, error: String) {
        val (ctx, nm) = ensureInitOrReturn() ?: return

        val existing = jobs.remove(jobId)
        val displayTitle = title ?: existing?.title ?: jobId
        val n = NotificationCompat.Builder(ctx, CHANNEL_ID)
            .setContentTitle(displayTitle)
            .setContentText(error.take(140))
            .setSmallIcon(android.R.drawable.stat_notify_error)
            .setGroup(GROUP_KEY)
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .build()

        mainHandler.post {
            nm.notify(jobIdToNotificationId(jobId), n)
            updateSummaryNotification()
        }
    }

    fun cancel(jobId: String) {
        val (_, nm) = ensureInitOrReturn() ?: return
        jobs.remove(jobId)
        mainHandler.post {
            nm.cancel(jobIdToNotificationId(jobId))
            updateSummaryNotification()
        }
    }

    fun buildSummaryNotification(activeJobIds: Collection<String>): Notification {
        val (ctx, _) = ensureInitOrReturn() ?: throw IllegalStateException("DownloadNotifications not initialized")

        val activeJobs = activeJobIds.mapNotNull { jobs[it] }
        val activeCount = activeJobs.size

        val avgProgress = if (activeJobs.isEmpty()) {
            0
        } else {
            val sum = activeJobs.sumOf { (it.progress ?: 0) }
            (sum / activeJobs.size).coerceIn(0, 100)
        }

        val title = if (activeCount == 1) {
            val j = activeJobs.firstOrNull()
            j?.title ?: "Downloading"
        } else {
            "Downloading $activeCount file(s)"
        }

        val text = if (activeCount == 1) {
            val j = activeJobs.firstOrNull()
            j?.let { summarizeLine(it) } ?: "$avgProgress%"
        } else {
            "$avgProgress% complete"
        }

        val pauseAllIntent = PendingIntent.getService(
            ctx, 2001,
            Intent(ctx, DownloadService::class.java).apply { action = DownloadService.ACTION_PAUSE_ALL },
            PendingIntent.FLAG_IMMUTABLE
        )

        val cancelAllIntent = PendingIntent.getService(
            ctx, 2002,
            Intent(ctx, DownloadService::class.java).apply {
                action = DownloadService.ACTION_CANCEL
                putExtra(DownloadService.EXTRA_JOB_ID, "__ALL__")
            },
            PendingIntent.FLAG_IMMUTABLE
        )

        return NotificationCompat.Builder(ctx, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(text)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setGroup(GROUP_KEY)
            .setGroupSummary(true)
            .setProgress(100, avgProgress, activeJobs.any { it.indeterminate })
            .addAction(android.R.drawable.ic_media_pause, "Pause All", pauseAllIntent)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "Cancel All", cancelAllIntent)
            .build()
    }

    fun updateSummaryNotification() {
        // Keep summary reasonably fresh (even if the foreground service isn't running).
        val (_, nm) = ensureInitOrReturn() ?: return
        val active = jobs.values
            .filter { it.kind == JobKind.DOWNLOAD }
            .map { it.jobId }

        if (active.isEmpty()) {
            nm.cancel(SUMMARY_NOTIFICATION_ID)
            return
        }
        val n = buildSummaryNotification(active)
        nm.notify(SUMMARY_NOTIFICATION_ID, n)
    }

    private fun buildJobNotification(ctx: Context, state: JobState, canPause: Boolean, ongoing: Boolean): Notification {
        val openAppIntent = PendingIntent.getActivity(
            ctx,
            1000,
            Intent(ctx, MainActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            },
            PendingIntent.FLAG_IMMUTABLE
        )

        val cancelIntent = PendingIntent.getBroadcast(
            ctx,
            jobIdToNotificationId(state.jobId) + 10,
            Intent(ctx, DownloadNotificationReceiver::class.java).apply {
                action = DownloadNotificationReceiver.ACTION_CANCEL
                putExtra(DownloadNotificationReceiver.EXTRA_JOB_ID, state.jobId)
                putExtra(DownloadNotificationReceiver.EXTRA_KIND, state.kind.name)
            },
            PendingIntent.FLAG_IMMUTABLE
        )

        val icon = when {
            state.kind == JobKind.CONVERT -> android.R.drawable.stat_notify_sync
            state.stage.lowercase().let { it.contains("embed") || it.contains("convert") || it.contains("process") || it.contains("merg") } -> 
                android.R.drawable.stat_notify_sync
            else -> android.R.drawable.stat_sys_download
        }

        val builder = NotificationCompat.Builder(ctx, CHANNEL_ID)
            .setContentTitle(state.title)
            .setContentText(summarizeLine(state))
            .setSmallIcon(icon)
            .setContentIntent(openAppIntent)
            .setOnlyAlertOnce(true)
            .setOngoing(ongoing)
            .setGroup(GROUP_KEY)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "Cancel", cancelIntent)

        if (canPause) {
            val pauseIntent = PendingIntent.getBroadcast(
                ctx,
                jobIdToNotificationId(state.jobId) + 11,
                Intent(ctx, DownloadNotificationReceiver::class.java).apply {
                    action = DownloadNotificationReceiver.ACTION_PAUSE
                    putExtra(DownloadNotificationReceiver.EXTRA_JOB_ID, state.jobId)
                    putExtra(DownloadNotificationReceiver.EXTRA_KIND, state.kind.name)
                },
                PendingIntent.FLAG_IMMUTABLE
            )
            builder.addAction(android.R.drawable.ic_media_pause, "Pause", pauseIntent)
        }

        when {
            state.indeterminate -> builder.setProgress(100, 0, true)
            state.progress != null -> builder.setProgress(100, state.progress!!.coerceIn(0, 100), false)
            else -> builder.setProgress(0, 0, false)
        }

        return builder.build()
    }

    private fun summarizeLine(state: JobState): String {
        val parts = mutableListOf<String>()
        if (state.stage.isNotBlank()) parts.add(state.stage)
        state.progress?.let { parts.add("$it%") }
        state.speedBps?.takeIf { it > 0 }?.let { parts.add(formatSpeed(it)) }
        state.etaSeconds?.takeIf { it >= 0 }?.let { parts.add("ETA ${formatEta(it)}") }
        return parts.joinToString(" • ")
    }

    private fun formatSpeed(bps: Long): String {
        val kb = 1024.0
        val mb = kb * 1024.0
        val gb = mb * 1024.0
        val v = bps.toDouble()
        return when {
            v >= gb -> String.format("%.2f GB/s", v / gb)
            v >= mb -> String.format("%.2f MB/s", v / mb)
            v >= kb -> String.format("%.1f KB/s", v / kb)
            else -> "$bps B/s"
        }
    }

    private fun formatEta(seconds: Long): String {
        val s = seconds.coerceAtLeast(0)
        val h = s / 3600
        val m = (s % 3600) / 60
        val sec = s % 60
        return when {
            h > 0 -> String.format("%d:%02d:%02d", h, m, sec)
            else -> String.format("%d:%02d", m, sec)
        }
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                CHANNEL_NAME,
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Shows download progress"
                setShowBadge(false)
            }
            notificationManager?.createNotificationChannel(channel)
        }
    }

    private fun jobIdToNotificationId(jobId: String): Int {
        // Keep stable-ish and non-zero.
        return 10000 + abs(jobId.hashCode() % 50000)
    }
}

object UpdateNotifications {
    private const val TAG = "UpdateNotifications"
    const val CHANNEL_ID = "comine_updates"
    private const val CHANNEL_NAME = "App Updates"
    private const val NOTIFICATION_ID = 9001

    const val ACTION_OPEN_UPDATES = "com.nichind.comine.OPEN_UPDATES"
    const val EXTRA_NAVIGATE_TO = "navigate_to"
    const val NAVIGATE_TO_UPDATES = "settings#app"

    @Volatile
    private var initialized = false
    private var appContext: Context? = null
    private var notificationManager: NotificationManager? = null

    fun init(context: Context) {
        if (initialized) return
        appContext = context.applicationContext
        notificationManager = appContext!!.getSystemService(NotificationManager::class.java)
        ensureChannel()
        initialized = true
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                CHANNEL_NAME,
                NotificationManager.IMPORTANCE_DEFAULT
            ).apply {
                description = "Notifications about new app updates"
                setShowBadge(true)
            }
            notificationManager?.createNotificationChannel(channel)
        }
    }

    fun showUpdateAvailable(version: String, isPreRelease: Boolean = false) {
        val ctx = appContext ?: return
        val nm = notificationManager ?: return

        val intent = Intent(ctx, MainActivity::class.java).apply {
            action = ACTION_OPEN_UPDATES
            putExtra(EXTRA_NAVIGATE_TO, NAVIGATE_TO_UPDATES)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        }

        val pendingIntent = PendingIntent.getActivity(
            ctx,
            NOTIFICATION_ID,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val title = "Update Available"
        val body = if (isPreRelease) {
            "Version $version (pre-release) is ready to install"
        } else {
            "Version $version is ready to install"
        }

        val notification = NotificationCompat.Builder(ctx, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentIntent(pendingIntent)
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .setCategory(NotificationCompat.CATEGORY_RECOMMENDATION)
            .build()

        nm.notify(NOTIFICATION_ID, notification)
    }

    fun dismiss() {
        notificationManager?.cancel(NOTIFICATION_ID)
    }
}

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
