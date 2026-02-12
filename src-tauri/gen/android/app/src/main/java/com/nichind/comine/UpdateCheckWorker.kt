package com.nichind.comine

import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

class UpdateCheckWorker(
    context: Context,
    params: WorkerParameters
) : CoroutineWorker(context, params) {

    companion object {
        private const val TAG = "UpdateCheckWorker"
        const val WORK_NAME = "update_check_work"
        private const val PREFS_NAME = "update_check_prefs"
        private const val PREF_LAST_NOTIFIED_VERSION = "last_notified_version"
    }

    private fun readSettingBool(key: String, default: Boolean): Boolean {
        return try {
            val file = File(applicationContext.filesDir, "settings.json")
            if (!file.exists()) return default
            val json = JSONObject(file.readText())
            if (json.has(key)) json.getBoolean(key) else default
        } catch (e: Exception) {
            Log.w(TAG, "Failed to read $key from settings.json, using default", e)
            default
        }
    }

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        try {
            Log.i(TAG, "Starting background update check")

            if (!readSettingBool("autoUpdate", true)) {
                return@withContext Result.success()
            }

            val allowPrereleases = readSettingBool("allowPreReleases", false)
            val currentVersion = getCurrentVersion()
            
            Log.i(TAG, "Current version: $currentVersion, allow prereleases: $allowPrereleases")
            
            val updateInfo = checkForUpdate(allowPrereleases)
            
            if (updateInfo != null) {
                val remoteVersion = updateInfo.getString("version")
                
                if (isNewerVersion(remoteVersion, currentVersion)) {
                    val notifPrefs = applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                    val lastNotified = notifPrefs.getString(PREF_LAST_NOTIFIED_VERSION, null)
                    
                    if (lastNotified != remoteVersion) {
                        UpdateNotifications.init(applicationContext)
                        UpdateNotifications.showUpdateAvailable(
                            version = remoteVersion,
                            isPreRelease = updateInfo.optBoolean("prerelease", false)
                        )
                        notifPrefs.edit().putString(PREF_LAST_NOTIFIED_VERSION, remoteVersion).apply()
                    }
                } else {
                }
            }

            Result.success()
        } catch (e: Exception) {
            Log.e(TAG, "Update check failed", e)
            Result.retry()
        }
    }

    private fun getCurrentVersion(): String {
        return try {
            val pInfo = applicationContext.packageManager.getPackageInfo(applicationContext.packageName, 0)
            pInfo.versionName ?: "0.0.0"
        } catch (e: Exception) {
            Log.e(TAG, "Failed to get current version", e)
            "0.0.0"
        }
    }

    private fun checkForUpdate(allowPrereleases: Boolean): JSONObject? {
        val url = if (allowPrereleases) {
            "https://api.github.com/repos/nichind/comine/releases"
        } else {
            "https://api.github.com/repos/nichind/comine/releases/latest"
        }

        val connection = URL(url).openConnection() as HttpURLConnection
        connection.apply {
            requestMethod = "GET"
            setRequestProperty("Accept", "application/vnd.github.v3+json")
            setRequestProperty("User-Agent", "comine-updater")
            connectTimeout = 15000
            readTimeout = 15000
        }

        try {
            if (connection.responseCode != HttpURLConnection.HTTP_OK) {
                Log.e(TAG, "GitHub API returned: ${connection.responseCode}")
                return null
            }

            val response = connection.inputStream.bufferedReader().readText()
            
            return if (allowPrereleases) {
                val releases = JSONArray(response)
                if (releases.length() > 0) {
                    val release = releases.getJSONObject(0)
                    JSONObject().apply {
                        put("version", release.getString("tag_name").removePrefix("v"))
                        put("prerelease", release.getBoolean("prerelease"))
                        put("body", release.optString("body", ""))
                    }
                } else null
            } else {
                val release = JSONObject(response)
                JSONObject().apply {
                    put("version", release.getString("tag_name").removePrefix("v"))
                    put("prerelease", release.getBoolean("prerelease"))
                    put("body", release.optString("body", ""))
                }
            }
        } finally {
            connection.disconnect()
        }
    }

    private fun isNewerVersion(remote: String, local: String): Boolean {
        try {
            val parseVersion = { v: String ->
                val parts = v.split("-", limit = 2)
                val versionParts = parts[0].split(".").map { it.toIntOrNull() ?: 0 }
                val preRelease = if (parts.size > 1) parts[1] else null
                Pair(versionParts, preRelease)
            }

            val (remoteParts, remotePre) = parseVersion(remote)
            val (localParts, localPre) = parseVersion(local)

            for (i in 0 until maxOf(remoteParts.size, localParts.size)) {
                val rv = remoteParts.getOrElse(i) { 0 }
                val lv = localParts.getOrElse(i) { 0 }
                if (rv > lv) return true
                if (rv < lv) return false
            }

            if (localPre != null && remotePre == null) return true
            if (localPre != null && remotePre != null) return remotePre > localPre

            return false
        } catch (e: Exception) {
            Log.e(TAG, "Failed to compare versions", e)
            return false
        }
    }

    class Settings(context: Context) {
        private val prefs: SharedPreferences = 
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

        fun clearLastNotifiedVersion() {
            prefs.edit().remove(PREF_LAST_NOTIFIED_VERSION).apply()
        }
    }
}
