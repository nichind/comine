package com.nichind.comine

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat

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
