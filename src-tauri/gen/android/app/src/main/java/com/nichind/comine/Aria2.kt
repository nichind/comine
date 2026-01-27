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

        val args = mutableListOf("-d", outputDir).apply {
            outputFile?.let { add("-o"); add(it) }
            add("-x"); add(connections.toString())
            add("-s"); add(splits.toString())
            add("-k"); add(minSplitSize)
            addAll(listOf("--continue=true", "--file-allocation=none", "--auto-file-renaming=false", "--allow-overwrite=true", "--show-console-readout=true", "--summary-interval=0"))
            if (speedLimit > 0) { add("--max-download-limit"); add("${speedLimit / 1024}K") }
            proxy?.let { add("--all-proxy"); add(it) }
            if (isTorrent) addAll(listOf("--listen-port", "6881-6999", "--dht-listen-port", "6881-6999", "--enable-dht=true", "--bt-enable-lpd=true", "--seed-ratio", "0.0"))
            add(url)
        }

        Log.d(TAG, "Running: $aria2Path ${args.joinToString(" ")}")
        RustBridge.notifyStarted(jobId, title)

        return try {
            val process = ProcessBuilder(listOf(aria2Path) + args).redirectErrorStream(true).start()
            runningProcesses[jobId] = process
            var outputPath: String? = null
            var lastProgressTime = 0L

            process.inputStream.bufferedReader().forEachLine { line ->
                if (!runningProcesses.containsKey(jobId)) {
                    return@forEachLine
                }
                if (line.isBlank()) return@forEachLine
                Log.d(TAG, "[$jobId]: $line")

                Regex("""\[#\w+\s+[\d.]+\w+/[\d.]+\w+\((\d+)%\)""").find(line)?.let { m ->
                    val now = System.currentTimeMillis()
                    if (now - lastProgressTime >= 100) {
                        val percent = m.groupValues[1].toIntOrNull() ?: 0
                        onProgress?.invoke(percent)
                        RustBridge.notifyProgress(jobId, percent.toFloat())
                        lastProgressTime = now
                    }
                }

                if (line.contains("Download complete:")) outputPath = line.substringAfter("Download complete:").trim()
            }

            runningProcesses.remove(jobId)
            val exitCode = process.waitFor()

            if (exitCode != 0 && !runningProcesses.containsKey(jobId)) {
                ExecuteResult.Cancelled
            } else if (exitCode == 0) {
                ExecuteResult.Success(outputPath ?: "$outputDir/${outputFile ?: "download"}", title)
            } else {
                ExecuteResult.Failed("aria2 exited with code $exitCode")
            }
        } catch (e: Exception) {
            runningProcesses.remove(jobId)
            Log.e(TAG, "aria2 failed", e)
            ExecuteResult.Failed(e.message ?: "Unknown error")
        }
    }

    sealed class ExecuteResult {
        data class Success(val outputPath: String, val title: String) : ExecuteResult()
        data class Failed(val error: String) : ExecuteResult()
        object Cancelled : ExecuteResult()
    }
}