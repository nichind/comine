package com.nichind.comine

import android.content.Context
import android.util.Log
import java.io.File
import java.util.concurrent.ConcurrentHashMap

object FFmpegExecutor {
    private const val TAG = "FFmpegExecutor"

    private val runningProcesses = ConcurrentHashMap<String, Process>()
    private val cancelledJobs = ConcurrentHashMap.newKeySet<String>()

    private var ffmpegBinaryPath: String? = null
    private var ldLibraryPath: String? = null
    private var binDir: String? = null

    fun init(context: Context) {
        val baseDir = File(context.noBackupFilesDir, "youtubedl-android")
        val packagesDir = File(baseDir, "packages")
        val pythonDir = File(packagesDir, "python")
        val ffmpegDir = File(packagesDir, "ffmpeg")
        val aria2cDir = File(packagesDir, "aria2c")
        
        binDir = context.applicationInfo.nativeLibraryDir
        ffmpegBinaryPath = File(binDir!!, "libffmpeg.so").absolutePath

        // Mirror youtubedl-android's environment setup.
        ldLibraryPath = "${pythonDir.absolutePath}/usr/lib:${ffmpegDir.absolutePath}/usr/lib:${aria2cDir.absolutePath}/usr/lib"
        
        Log.i(TAG, "FFmpeg binary: $ffmpegBinaryPath")
        Log.i(TAG, "LD_LIBRARY_PATH: $ldLibraryPath")
        Log.i(TAG, "Python libs exists: ${File(pythonDir, "usr/lib").exists()}")
        Log.i(TAG, "FFmpeg libs exists: ${File(ffmpegDir, "usr/lib").exists()}")
    }

    fun cancel(jobId: String): Boolean {
        val process = runningProcesses.remove(jobId)
        if (process != null) {
            Log.i(TAG, "Cancelling ffmpeg job $jobId")
            cancelledJobs.add(jobId)
            process.destroy()
            return true
        }
        return false
    }

    fun execute(
        context: Context,
        jobId: String,
        args: List<String>,
        totalDuration: Double? = null,
        onProgress: ((Float, String?) -> Unit)? = null
    ): ExecuteResult {
        if (ffmpegBinaryPath == null) {
            init(context)
        }
        
        val ffmpegPath = ffmpegBinaryPath!!
        
        if (!File(ffmpegPath).exists()) {
            Log.e(TAG, "FFmpeg binary not found at $ffmpegPath")
            return ExecuteResult.Failed("FFmpeg binary not found")
        }

        val fullArgs = mutableListOf<String>()
        fullArgs.add("-progress")
        fullArgs.add("pipe:1")
        fullArgs.add("-stats_period")
        fullArgs.add("0.5")
        fullArgs.addAll(args)

        Log.i(TAG, "Running FFmpeg: $ffmpegPath ${fullArgs.joinToString(" ")}")
        Log.i(TAG, "LD_LIBRARY_PATH: $ldLibraryPath")
        Log.i(TAG, "Total duration: $totalDuration seconds")

        return try {
            val processBuilder = ProcessBuilder(listOf(ffmpegPath) + fullArgs)
                .redirectErrorStream(true)
            
            processBuilder.environment().apply {
                this["LD_LIBRARY_PATH"] = ldLibraryPath
                this["PATH"] = System.getenv("PATH") + ":" + binDir
            }

            processBuilder.redirectInput(ProcessBuilder.Redirect.PIPE)
            
            val process = processBuilder.start()
            process.outputStream.close()
            
            runningProcesses[jobId] = process
            val allLines = mutableListOf<String>()

            var currentTimeMs: Long = 0
            var currentSpeed: String? = null
            val outTimeMsRegex = Regex("""out_time_ms=(\d+)""")
            val speedRegex = Regex("""speed=\s*([0-9.]+)x""")

            process.inputStream.bufferedReader().forEachLine { line ->
                if (cancelledJobs.contains(jobId)) return@forEachLine

                allLines.add(line)
                if (allLines.size > 20) allLines.removeAt(0)

                outTimeMsRegex.find(line)?.let { match ->
                    currentTimeMs = match.groupValues[1].toLongOrNull() ?: 0
                }

                speedRegex.find(line)?.let { match ->
                    currentSpeed = "${match.groupValues[1]}x"
                }

                if (currentTimeMs > 0) {
                    val timeSecs = currentTimeMs / 1_000_000.0
                    val percent = if (totalDuration != null && totalDuration > 0) {
                        ((timeSecs / totalDuration) * 100.0).coerceIn(0.0, 99.0).toFloat()
                    } else {
                        0f
                    }
                    
                    Log.d(TAG, "[$jobId] Progress: ${percent.toInt()}% (${timeSecs}s / ${totalDuration}s) speed=$currentSpeed")
                    
                    if (percent > 0) {
                        onProgress?.invoke(percent, currentSpeed)
                    }
                }
            }

            runningProcesses.remove(jobId)
            val exitCode = process.waitFor()
            val wasCancelled = cancelledJobs.remove(jobId)
            
            when {
                wasCancelled -> ExecuteResult.Cancelled
                exitCode == 0 -> ExecuteResult.Success
                else -> {
                    val errorLines = allLines.filter { 
                        it.contains("Error", ignoreCase = true) || 
                        it.contains("Invalid", ignoreCase = true) ||
                        it.contains("No such file", ignoreCase = true) ||
                        it.contains("does not contain", ignoreCase = true) ||
                        it.contains("not found", ignoreCase = true) ||
                        it.contains("Unsupported", ignoreCase = true) ||
                        it.contains("Permission denied", ignoreCase = true)
                    }
                    val errorMsg = when {
                        errorLines.isNotEmpty() -> errorLines.takeLast(3).joinToString("; ")
                        allLines.isNotEmpty() -> "FFmpeg failed (code $exitCode): ${allLines.takeLast(3).joinToString("; ")}"
                        else -> "FFmpeg exited with code $exitCode"
                    }
                    Log.e(TAG, "FFmpeg failed: $errorMsg")
                    ExecuteResult.Failed(errorMsg)
                }
            }
        } catch (e: Exception) {
            runningProcesses.remove(jobId)
            cancelledJobs.remove(jobId)
            Log.e(TAG, "FFmpeg failed", e)
            ExecuteResult.Failed(e.message ?: "Unknown error")
        }
    }

    fun getDuration(context: Context, filePath: String): Double? {
        if (ffmpegBinaryPath == null) {
            init(context)
        }
        
        val ffmpegPath = ffmpegBinaryPath!!
        Log.d(TAG, "Getting duration for: $filePath")

        return try {
            val processBuilder = ProcessBuilder(listOf(ffmpegPath, "-i", filePath))
                .redirectErrorStream(true)

            processBuilder.environment().apply {
                this["LD_LIBRARY_PATH"] = ldLibraryPath
                this["PATH"] = System.getenv("PATH") + ":" + binDir
            }
            
            val process = processBuilder.start()
            process.outputStream.close()
            
            var duration: Double? = null
            process.inputStream.bufferedReader().forEachLine { line ->
                Log.d(TAG, "Duration probe: $line")
                if (line.contains("Duration:")) {
                    val durationMatch = Regex("""Duration:\s*(\d+):(\d+):(\d+(?:\.\d+)?)""").find(line)
                    if (durationMatch != null) {
                        val (hours, minutes, seconds) = durationMatch.destructured
                        duration = hours.toDouble() * 3600 + minutes.toDouble() * 60 + seconds.toDouble()
                        Log.i(TAG, "Found duration: $duration seconds")
                    }
                }
            }
            process.waitFor()
            Log.i(TAG, "Final duration: $duration")
            duration
        } catch (e: Exception) {
            Log.e(TAG, "Failed to get duration", e)
            null
        }
    }

    private fun parseTimeToSeconds(timeStr: String): Double? {
        return try {
            val parts = timeStr.split(":")
            when (parts.size) {
                3 -> {
                    val hours = parts[0].toDoubleOrNull() ?: 0.0
                    val minutes = parts[1].toDoubleOrNull() ?: 0.0
                    val seconds = parts[2].toDoubleOrNull() ?: 0.0
                    hours * 3600 + minutes * 60 + seconds
                }
                2 -> {
                    val minutes = parts[0].toDoubleOrNull() ?: 0.0
                    val seconds = parts[1].toDoubleOrNull() ?: 0.0
                    minutes * 60 + seconds
                }
                1 -> parts[0].toDoubleOrNull()
                else -> null
            }
        } catch (e: Exception) {
            null
        }
    }

    sealed class ExecuteResult {
        object Success : ExecuteResult()
        data class Failed(val error: String) : ExecuteResult()
        object Cancelled : ExecuteResult()
    }
}
