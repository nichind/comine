package com.nichind.comine

import android.content.Context
import android.util.Log
import java.io.File
import java.util.concurrent.ConcurrentHashMap

object Aria2 {
    private const val TAG = "Aria2"

    private val runningProcesses = ConcurrentHashMap<String, Process>()

    fun cancel(jobId: String): Boolean {
        val process = runningProcesses.remove(jobId)
        if (process != null) {
            Log.i(TAG, "Cancelling aria2 job $jobId")
            process.destroy()
            return true
        }
        return false
    }

    fun execute(
        context: Context,
        jobId: String,
        url: String,
        outputDir: String,
        outputFile: String?,
        connections: Int = 8,
        splits: Int = 8,
        minSplitSize: String = "1M",
        speedLimit: Long = 0,
        proxy: String? = null,
        isTorrent: Boolean = false,
        onProgress: ((Int) -> Unit)? = null
    ): ExecuteResult {
        val aria2Path = File(context.applicationInfo.nativeLibraryDir, "libaria2c.so").absolutePath
        val title = outputFile ?: url.substringAfterLast('/').substringBefore('?').ifBlank { "aria2 download" }

        runCatching { DownloadNotifications.init(context) }
        DownloadNotifications.upsert(
            jobId = jobId,
            kind = DownloadNotifications.JobKind.DOWNLOAD,
            title = title,
            stage = "Downloading",
            progress = 0,
            indeterminate = true,
            canPause = false,
            ongoing = true
        )

        val args = mutableListOf("-d", outputDir).apply {
            outputFile?.let { add("-o"); add(it) }
            add("-x"); add(connections.toString())
            add("-s"); add(splits.toString())
            add("-k"); add(minSplitSize)
            addAll(listOf(
                "--continue=true",
                "--file-allocation=none",
                "--auto-file-renaming=false",
                "--allow-overwrite=true",
                "--show-console-readout=true",
                "--summary-interval=1",
                "--check-certificate=false",
                "--console-log-level=notice"
            ))
            if (speedLimit > 0) { add("--max-download-limit"); add("${speedLimit / 1024}K") }
            proxy?.let { add("--all-proxy"); add(it) }
            if (isTorrent) addAll(listOf("--listen-port", "6881-6999", "--dht-listen-port", "6881-6999", "--enable-dht=true", "--bt-enable-lpd=true", "--seed-ratio", "0.0"))
            add(url)
        }

        Log.d(TAG, "Running: $aria2Path ${args.joinToString(" ")}")
        
        File(outputDir).takeIf { !it.exists() }?.mkdirs()
        
        if (!File(aria2Path).exists()) {
            val error = "aria2 binary not found"
            DownloadNotifications.fail(jobId, title = title, error = error)
            return ExecuteResult.Failed(error)
        }
        
        RustBridge.notifyStarted(jobId, title)

        return try {
            val process = ProcessBuilder(listOf(aria2Path) + args).redirectErrorStream(true).start()
            runningProcesses[jobId] = process
            var outputPath: String? = null
            var lastProgressTime = 0L
            
            val progressRegex = Regex("""\[#\w+\s+[\d.]+\w+/[\d.]+\w+(?:\((\d+)%\))?""")
            val percentRegex = Regex("""\((\d+)%\)""")
            val completeRegex = Regex("""(?:Download complete:|download completed\.)?\s*(/\S+|[A-Za-z]:\\\S+)""", RegexOption.IGNORE_CASE)

            process.inputStream.bufferedReader().forEachLine { line ->
                if (!runningProcesses.containsKey(jobId)) {
                    return@forEachLine
                }
                if (line.isBlank()) return@forEachLine
                Log.d(TAG, "[$jobId]: $line")

                val percent = progressRegex.find(line)?.groupValues?.get(1)?.toIntOrNull()
                    ?: percentRegex.find(line)?.groupValues?.get(1)?.toIntOrNull()
                
                if (percent != null) {
                    val now = System.currentTimeMillis()
                    if (now - lastProgressTime >= 100) {
                        onProgress?.invoke(percent)
                        RustBridge.notifyProgress(jobId, percent.toFloat())
                        DownloadNotifications.upsert(
                            jobId = jobId,
                            kind = DownloadNotifications.JobKind.DOWNLOAD,
                            title = title,
                            stage = "Downloading",
                            progress = percent,
                            indeterminate = false,
                            canPause = false,
                            ongoing = true
                        )
                        lastProgressTime = now
                    }
                }

                if (line.contains("Download complete") || line.contains("download completed")) {
                    completeRegex.find(line)?.groupValues?.get(1)?.let { path ->
                        if (File(path).exists()) outputPath = path
                    }
                }
                if (outputPath == null && line.startsWith(outputDir)) {
                    val potentialPath = line.trim()
                    if (File(potentialPath).exists()) outputPath = potentialPath
                }
            }

            val wasCancelled = !runningProcesses.containsKey(jobId)
            runningProcesses.remove(jobId)
            val exitCode = process.waitFor()
            
            if (outputPath == null) {
                val expectedFile = File(outputDir, outputFile ?: url.substringAfterLast('/').substringBefore('?'))
                if (expectedFile.exists() && expectedFile.length() > 0) {
                    outputPath = expectedFile.absolutePath
                }
            }

            Log.i(TAG, "aria2 finished: jobId=$jobId, exitCode=$exitCode, wasCancelled=$wasCancelled, outputPath=$outputPath")

            if (wasCancelled) {
                DownloadNotifications.cancel(jobId)
                ExecuteResult.Cancelled
            } else if (exitCode == 0 || (outputPath?.let { File(it).exists() } == true)) {
                val finalPath = outputPath ?: "$outputDir/${outputFile ?: "download"}"
                DownloadNotifications.complete(
                    jobId = jobId,
                    title = title,
                    info = "Saved",
                    outputPath = finalPath
                )
                ExecuteResult.Success(finalPath, title)
            } else {
                val error = "aria2 exited with code $exitCode"
                Log.e(TAG, error)
                DownloadNotifications.fail(jobId, title = title, error = error)
                ExecuteResult.Failed(error)
            }
        } catch (e: Exception) {
            runningProcesses.remove(jobId)
            Log.e(TAG, "aria2 failed", e)
            DownloadNotifications.fail(jobId, title = title, error = e.message ?: "Unknown error")
            ExecuteResult.Failed(e.message ?: "Unknown error")
        }
    }

    sealed class ExecuteResult {
        data class Success(val outputPath: String, val title: String) : ExecuteResult()
        data class Failed(val error: String) : ExecuteResult()
        object Cancelled : ExecuteResult()
    }
}