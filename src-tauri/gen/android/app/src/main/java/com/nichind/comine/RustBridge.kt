package com.nichind.comine

import android.util.Log

object RustBridge {
    private const val TAG = "RustBridge"
    @Volatile private var initialized = false
    @Volatile private var libraryLoaded = false

    init {
        try {
            System.loadLibrary("comine_lib")
            libraryLoaded = true
            Log.i(TAG, "Native library loaded successfully")
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "Failed to load native library: ${e.message}")
        }
    }

    // Rust keeps a global reference to the Activity for JNI callbacks.
    private external fun initRustJniWithActivity(activity: MainActivity)

    fun initialize() {
        if (!libraryLoaded) {
            Log.e(TAG, "Cannot initialize JNI - native library not loaded")
            return
        }
        if (initialized) return
        
        val activity = MainActivity.currentInstance()
        if (activity == null) {
            Log.e(TAG, "Cannot initialize JNI - MainActivity not available")
            return
        }
        
        try {
            initRustJniWithActivity(activity)
            initialized = true
            Log.i(TAG, "JNI bridge initialized with MainActivity")
        } catch (e: Exception) {
            Log.e(TAG, "JNI init failed: ${e.message}", e)
        }
    }
    
    fun isReady(): Boolean = initialized && libraryLoaded

    @JvmStatic external fun nativeOnDownloadStarted(jobId: String, title: String)
    @JvmStatic external fun nativeOnDownloadProgress(jobId: String, progress: Float, speedBps: Long, etaSeconds: Long, downloadedBytes: Long, totalBytes: Long)
    @JvmStatic external fun nativeOnDownloadCompleted(jobId: String, outputPath: String, title: String?, thumbnail: String?)
    @JvmStatic external fun nativeOnDownloadFailed(jobId: String, error: String)
    @JvmStatic external fun nativeOnDownloadCancelled(jobId: String)
    @JvmStatic external fun nativeOnDownloadPaused(jobId: String)

    @JvmStatic external fun nativeOnConvertProgress(jobId: String, progress: Float, speed: String?)
    @JvmStatic external fun nativeOnConvertCompleted(jobId: String, outputPath: String, filesize: Long, extension: String, duration: Long?)
    @JvmStatic external fun nativeOnConvertFailed(jobId: String, error: String)

    fun notifyStarted(jobId: String, title: String) {
        if (!initialized) {
            Log.w(TAG, "notifyStarted: JNI not initialized, skipping for job $jobId")
            return
        }
        runCatching { nativeOnDownloadStarted(jobId, title) }
            .onFailure { Log.e(TAG, "notifyStarted failed: ${it.message}") }
    }
    
    fun notifyProgress(jobId: String, progress: Float, speedBps: Long = 0, etaSeconds: Long = -1, downloadedBytes: Long = 0, totalBytes: Long = 0) {
        if (!initialized) return
        runCatching { nativeOnDownloadProgress(jobId, progress, speedBps, etaSeconds, downloadedBytes, totalBytes) }
            .onFailure { Log.e(TAG, "notifyProgress failed: ${it.message}") }
    }
    
    fun notifyCompleted(jobId: String, outputPath: String, title: String? = null, thumbnail: String? = null) {
        if (!initialized) {
            Log.w(TAG, "notifyCompleted: JNI not initialized, skipping for job $jobId")
            return
        }
        runCatching { nativeOnDownloadCompleted(jobId, outputPath, title, thumbnail) }
            .onFailure { Log.e(TAG, "notifyCompleted failed: ${it.message}") }
    }
    
    fun notifyFailed(jobId: String, error: String) {
        if (!initialized) {
            Log.w(TAG, "notifyFailed: JNI not initialized, skipping for job $jobId")
            return
        }
        runCatching { nativeOnDownloadFailed(jobId, error) }
            .onFailure { Log.e(TAG, "notifyFailed failed: ${it.message}") }
    }
    
    fun notifyCancelled(jobId: String) {
        if (!initialized) {
            Log.w(TAG, "notifyCancelled: JNI not initialized, skipping for job $jobId")
            return
        }
        runCatching { nativeOnDownloadCancelled(jobId) }
            .onFailure { Log.e(TAG, "notifyCancelled failed: ${it.message}") }
    }
    
    fun notifyPaused(jobId: String) {
        if (!initialized) {
            Log.w(TAG, "notifyPaused: JNI not initialized, skipping for job $jobId")
            return
        }
        runCatching { nativeOnDownloadPaused(jobId) }
            .onFailure { Log.e(TAG, "notifyPaused failed: ${it.message}") }
    }

    fun notifyConvertProgress(jobId: String, progress: Float, speed: String? = null) {
        if (!initialized) return
        runCatching { nativeOnConvertProgress(jobId, progress, speed) }
            .onFailure { Log.e(TAG, "notifyConvertProgress failed: ${it.message}") }
    }

    fun notifyConvertCompleted(jobId: String, outputPath: String, filesize: Long, extension: String, duration: Long? = null) {
        if (!initialized) {
            Log.w(TAG, "notifyConvertCompleted: JNI not initialized, skipping for job $jobId")
            return
        }
        runCatching { nativeOnConvertCompleted(jobId, outputPath, filesize, extension, duration ?: -1L) }
            .onFailure { Log.e(TAG, "notifyConvertCompleted failed: ${it.message}") }
    }

    fun notifyConvertFailed(jobId: String, error: String) {
        if (!initialized) {
            Log.w(TAG, "notifyConvertFailed: JNI not initialized, skipping for job $jobId")
            return
        }
        runCatching { nativeOnConvertFailed(jobId, error) }
            .onFailure { Log.e(TAG, "notifyConvertFailed failed: ${it.message}") }
    }
}