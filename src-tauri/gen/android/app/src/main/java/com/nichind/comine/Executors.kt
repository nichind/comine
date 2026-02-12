package com.nichind.comine

import android.app.Application
import android.content.Context
import android.os.Environment
import android.util.Log
import com.yausername.aria2c.Aria2c
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.ConcurrentHashMap

private fun looksLikeNetscapeCookieText(s: String): Boolean {
    val t = s.trimStart()
    return s.contains('\n') || t.startsWith("# Netscape")
}

object NativeLibPaths {
    private const val TAG = "NativeLibPaths"
    
    @Volatile var initialized = false; private set
    
    lateinit var binDir: String; private set
    lateinit var ldLibraryPath: String; private set
    var sslCertPath: String? = null; private set
    
    fun init(context: Context) {
        if (initialized) return
        
        val baseDir = File(context.noBackupFilesDir, "youtubedl-android")
        val packagesDir = File(baseDir, "packages")
        val pythonDir = File(packagesDir, "python")
        val ffmpegDir = File(packagesDir, "ffmpeg")
        val aria2cDir = File(packagesDir, "aria2c")
        
        binDir = context.applicationInfo.nativeLibraryDir
        ldLibraryPath = "${binDir}:${pythonDir.absolutePath}/usr/lib:${ffmpegDir.absolutePath}/usr/lib:${aria2cDir.absolutePath}/usr/lib"
        
        val sslCertFile = File(packagesDir, "python/usr/etc/tls/cert.pem")
        if (sslCertFile.exists()) {
            sslCertPath = sslCertFile.absolutePath
        }
        
        initialized = true
    }
}

object YtDlp {
    private const val TAG = "YtDlp"

    @Volatile private var app: Application? = null

    @Volatile var initialized = false; private set
    @Volatile var ffmpegAvailable = false; private set
    @Volatile var aria2Available = false; private set

    fun init(app: Application, onReady: (() -> Unit)? = null) {
        this.app = app
        Thread {
            runCatching { YoutubeDL.getInstance().init(app); initialized = true; Log.i(TAG, "yt-dlp ready") }
            runCatching { FFmpeg.getInstance().init(app); ffmpegAvailable = true; Log.i(TAG, "ffmpeg ready") }
            runCatching { Aria2c.getInstance().init(app); aria2Available = true; Log.i(TAG, "aria2 ready") }
            if (initialized) {
                runCatching {
                    val status = YoutubeDL.getInstance().updateYoutubeDL(app, YoutubeDL.UpdateChannel.MASTER)
                    Log.i(TAG, "yt-dlp master update: $status")
                }
            }
            onReady?.invoke()
        }.start()
    }

    fun getVersion(app: Application): String = runCatching { YoutubeDL.getInstance().versionName(app) ?: "" }.getOrDefault("")

    fun updateChannel(app: Application, channel: String): String {
        val updateChannel = when (channel.lowercase()) {
            "nightly" -> YoutubeDL.UpdateChannel.MASTER
            "master" -> YoutubeDL.UpdateChannel.MASTER
            else -> YoutubeDL.UpdateChannel.STABLE
        }
        val status = YoutubeDL.getInstance().updateYoutubeDL(app, updateChannel)
        Log.i(TAG, "yt-dlp update to $channel: $status")
        return getVersion(app)
    }

    fun cancel(jobId: String): Boolean {
        runCatching { FFmpegExecutor.cancel("$jobId:thumb") }
        runCatching { FFmpegExecutor.cancel("$jobId:thumbimg") }
        return runCatching { YoutubeDL.getInstance().destroyProcessById(jobId) }.getOrDefault(false)
    }

    @JvmStatic
    fun resolve(url: String, flatPlaylist: Boolean, youtubePlayerClient: String?): ResolveResult {
        if (!initialized) return ResolveResult.Failed("yt-dlp not initialized")

        return try {
            val request = YoutubeDLRequest(url).apply {
                addOption("--dump-json")
                addOption("--no-download")
                addOption("--no-warnings")
                addOption("--ignore-errors")
                addOption("--encoding", "utf-8")
                
                if (flatPlaylist) {
                    addOption("--flat-playlist")
                } else {
                    addOption("--no-playlist")
                }
                
                if (!youtubePlayerClient.isNullOrBlank() && youtubePlayerClient != "null") {
                    addOption("--extractor-args", "youtube:player_client=$youtubePlayerClient")
                }
            }

            val response = YoutubeDL.getInstance().execute(request)
            if (response.exitCode == 0) {
                ResolveResult.Success(response.out ?: "")
            } else {
                ResolveResult.Failed(response.err ?: "yt-dlp failed with exit code ${response.exitCode}")
            }
        } catch (e: Exception) {
            Log.e(TAG, "resolve failed", e)
            ResolveResult.Failed(e.message ?: "unknown error")
        }
    }

    sealed class ResolveResult {
        data class Success(val output: String) : ResolveResult()
        data class Failed(val error: String) : ResolveResult()
    }

    private fun applyArgsFromJson(request: YoutubeDLRequest, argsJson: String, jobId: String): File? {
        var cookieTempFile: File? = null
        
        val groups = JSONArray(argsJson)
        for (i in 0 until groups.length()) {
            val g = groups.optJSONArray(i) ?: continue
            val key = g.optString(0, "").takeIf { it.isNotBlank() } ?: continue
            val rawVal = g.optString(1, "")
            if (g.length() >= 2) {
                var value = rawVal
                if (key == "--cookies" && !value.isBlank() && looksLikeNetscapeCookieText(value)) {
                    val appCtx = app
                    if (appCtx != null) {
                        val f = File(appCtx.cacheDir, "comine_cookies_${jobId}_${System.currentTimeMillis()}.txt")
                        runCatching {
                            f.parentFile?.mkdirs()
                            f.writeText(value, Charsets.UTF_8)
                        }.onSuccess {
                            cookieTempFile = f
                            value = f.absolutePath
                        }
                    }
                }
                request.addOption(key, value)
            } else {
                request.addOption(key)
            }
        }
        return cookieTempFile
    }

    @JvmStatic
    fun resolveJson(url: String, argsJson: String): ResolveResult {
        if (!initialized) return ResolveResult.Failed("yt-dlp not initialized")

        var cookieTempFile: File? = null
        return try {
            val request = YoutubeDLRequest(url)
            cookieTempFile = applyArgsFromJson(request, argsJson, "resolve")

            val response = YoutubeDL.getInstance().execute(request)
            if (response.exitCode == 0) {
                ResolveResult.Success(response.out ?: "")
            } else {
                ResolveResult.Failed(response.err ?: "yt-dlp failed with exit code ${response.exitCode}")
            }
        } catch (e: Exception) {
            Log.e(TAG, "resolveJson failed", e)
            ResolveResult.Failed(e.message ?: "unknown error")
        } finally {
            runCatching { cookieTempFile?.delete() }
        }
    }

    @JvmStatic
    fun resolveStreaming(resolveId: String, url: String, argsJson: String) {
        if (!initialized) {
            RustBridge.notifyFailed(resolveId, "yt-dlp not initialized")
            return
        }
        Thread {
            var cookieTempFile: File? = null
            try {
                val request = YoutubeDLRequest(url)
                cookieTempFile = applyArgsFromJson(request, argsJson, resolveId)

                val response = YoutubeDL.getInstance().execute(request, resolveId) { _, _, line ->
                    if (!line.isNullOrBlank()) {
                        RustBridge.notifyResolveOutput(resolveId, line)
                    }
                }

                if (response.exitCode == 0 || (response.out?.isNotBlank() == true)) {
                    RustBridge.notifyCompleted(resolveId, "", null, null)
                } else {
                    RustBridge.notifyFailed(resolveId, response.err ?: "exitCode=${response.exitCode}")
                }
            } catch (e: YoutubeDL.CanceledException) {
                RustBridge.notifyCancelled(resolveId)
            } catch (e: Exception) {
                Log.e(TAG, "resolveStreaming failed", e)
                RustBridge.notifyFailed(resolveId, e.message ?: "unknown error")
            } finally {
                runCatching { cookieTempFile?.delete() }
            }
        }.start()
    }

    @JvmStatic
    fun cancelResolve(resolveId: String): Boolean {
        return runCatching { YoutubeDL.getInstance().destroyProcessById(resolveId) }.getOrDefault(false)
    }

    fun execute(
        jobId: String,
        url: String,
        argsJson: String,
        outputDir: String? = null,
        onProgress: ((progress: Int, etaSeconds: Long?) -> Unit)? = null,
        onRawOutput: ((String) -> Unit)? = null,
    ): ExecuteResult {
        if (!initialized) return ExecuteResult.Failed("yt-dlp not initialized")

        var cookieTempFile: File? = null
        return try {
            val dlDir = if (!outputDir.isNullOrBlank()) {
                File(outputDir)
            } else {
                File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Comine")
            }
            if (!dlDir.exists()) dlDir.mkdirs()

            val request = YoutubeDLRequest(url)
            cookieTempFile = applyArgsFromJson(request, argsJson, jobId)

            var lastFilePath: String? = null
            var lastTitle: String? = null
            var lastThumbnail: String? = null

            fun tryCaptureMarkers(line: String) {
                when {
                    line.startsWith(">>>FILEPATH") -> {
                        lastFilePath = line.substringAfter(':', "").trim().takeIf { it.isNotBlank() }
                    }
                    line.startsWith(">>>TITLE") -> {
                        lastTitle = line.substringAfter(':', "").trim().takeIf { it.isNotBlank() }
                    }
                    line.startsWith(">>>THUMBNAIL") -> {
                        lastThumbnail = line.substringAfter(':', "").trim().takeIf { it.isNotBlank() }
                    }
                }
            }

            val response = YoutubeDL.getInstance().execute(request, jobId) { p, eta, line ->
                    if (!line.isNullOrBlank()) {
                        tryCaptureMarkers(line)
                        onRawOutput?.invoke(line)
                    }
                    val progressInt = when {
                        p == null -> null
                        p.isNaN() -> null
                        else -> p.toInt().coerceIn(0, 100)
                    }
                    if (progressInt != null) {
                        onProgress?.invoke(progressInt, eta?.toString()?.toLongOrNull())
                    }
            }

            if (response.exitCode == 0) {
                ExecuteResult.Success(lastFilePath ?: dlDir.absolutePath, lastTitle, lastThumbnail)
            } else {
                val err = response.err ?: "exitCode=${response.exitCode}"
                ExecuteResult.Failed(err)
            }
        } catch (e: Exception) {
            ExecuteResult.Failed(e.message ?: "unknown")
        } finally {
            runCatching { cookieTempFile?.delete() }
        }
    }


    sealed class ExecuteResult {
        data class Success(val outputPath: String, val title: String? = null, val thumbnailUrl: String? = null) : ExecuteResult()
        data class Failed(val error: String) : ExecuteResult()
    }
}

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
        argsJson: String,
        onRawOutput: ((String) -> Unit)? = null
    ): ExecuteResult {
        NativeLibPaths.init(context)

        val aria2Path = File(NativeLibPaths.binDir, "libaria2c.so").absolutePath

        val args = mutableListOf<String>()
        val groups = JSONArray(argsJson)
        for (i in 0 until groups.length()) {
            val g = groups.optJSONArray(i) ?: continue
            for (j in 0 until g.length()) {
                val a = g.optString(j, "")
                if (a.isNotBlank()) args.add(a)
            }
        }

        NativeLibPaths.sslCertPath?.let { certPath ->
            args.add("--ca-certificate=$certPath")
        }

        val dIdx = args.indexOf("-d")
        if (dIdx >= 0 && dIdx + 1 < args.size) {
            runCatching { File(args[dIdx + 1]).takeIf { !it.exists() }?.mkdirs() }
        }

        val oIdx = args.indexOf("-o")
        val outDir = args.getOrNull(dIdx + 1)?.takeIf { it.isNotBlank() }
        val outName = args.getOrNull(oIdx + 1)?.takeIf { it.isNotBlank() }
        val outputPathGuess = when {
            outDir != null && outName != null -> File(outDir, outName).absolutePath
            outDir != null -> File(outDir).absolutePath
            else -> ""
        }
        
        if (!File(aria2Path).exists()) {
            val error = "aria2 binary not found"
            Log.e(TAG, error)
            return ExecuteResult.Failed(error)
        }

        return try {
            val processBuilder = ProcessBuilder(listOf(aria2Path) + args).redirectErrorStream(true)
            processBuilder.environment().apply {
                this["LD_LIBRARY_PATH"] = NativeLibPaths.ldLibraryPath
                this["PATH"] = System.getenv("PATH") + ":" + NativeLibPaths.binDir
                this["TERM"] = "xterm-256color" 
                NativeLibPaths.sslCertPath?.let { this["SSL_CERT_FILE"] = it }
                    ?: run { this["SSL_CERT_DIR"] = "/system/etc/security/cacerts" }
            }
            val process = processBuilder.start()
            runningProcesses[jobId] = process
            val outputLines = mutableListOf<String>()
            
            val inputStream = process.inputStream
            val byteBuffer = ByteArray(4096)
            val sb = StringBuilder()
            var readLen: Int
            val ansiRegex = Regex("\u001B\\[[;\\d]*[a-zA-Z]")

            while (inputStream.read(byteBuffer).also { readLen = it } != -1) {
                if (!runningProcesses.containsKey(jobId)) break
                if (readLen > 0) {
                    val chunk = String(byteBuffer, 0, readLen, Charsets.UTF_8)
                    for (c in chunk) {
                        if (c == '\r' || c == '\n') {
                            if (sb.isNotEmpty()) {
                                val rawLine = sb.toString()
                                sb.setLength(0)
                                
                                val cleanLine = rawLine.replace(ansiRegex, "").trim()
                                
                                if (cleanLine.isNotBlank()) {
                                    onRawOutput?.invoke(cleanLine)
                                    outputLines.add(cleanLine)
                                    if (outputLines.size > 20) outputLines.removeAt(0)
                                }
                            }
                        } else {
                            sb.append(c)
                        }
                    }
                }
            }
            if (sb.isNotEmpty()) {
                val line = sb.toString()
                outputLines.add(line)
                onRawOutput?.invoke(line)
            }

            val wasCancelled = !runningProcesses.containsKey(jobId)
            runningProcesses.remove(jobId)
            val exitCode = process.waitFor()

            if (wasCancelled) {
                ExecuteResult.Cancelled
            } else if (exitCode == 0) {
                ExecuteResult.Success(outputPathGuess)
            } else {
                val lastOutput = outputLines.takeLast(5).joinToString("; ")
                val error = "aria2 exited with code $exitCode: $lastOutput"
                Log.e(TAG, error)
                ExecuteResult.Failed(error)
            }
        } catch (e: Exception) {
            runningProcesses.remove(jobId)
            Log.e(TAG, "aria2 failed", e)
            ExecuteResult.Failed(e.message ?: "Unknown error")
        }
    }

    sealed class ExecuteResult {
        data class Success(val outputPath: String) : ExecuteResult()
        data class Failed(val error: String) : ExecuteResult()
        object Cancelled : ExecuteResult()
    }
}

object FFmpegExecutor {
    private const val TAG = "FFmpegExecutor"

    private val runningProcesses = ConcurrentHashMap<String, Process>()
    private val cancelledJobs = ConcurrentHashMap.newKeySet<String>()

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
        NativeLibPaths.init(context)
        
        val ffmpegPath = File(NativeLibPaths.binDir, "libffmpeg.so").absolutePath
        
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

        return try {
            val processBuilder = ProcessBuilder(listOf(ffmpegPath) + fullArgs)
                .redirectErrorStream(true)
            
            processBuilder.environment().apply {
                this["LD_LIBRARY_PATH"] = NativeLibPaths.ldLibraryPath
                this["PATH"] = System.getenv("PATH") + ":" + NativeLibPaths.binDir
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

            val inputStream = process.inputStream
            val byteBuffer = ByteArray(4096)
            val sb = StringBuilder()
            var readLen: Int
            
            while (inputStream.read(byteBuffer).also { readLen = it } != -1) {
                if (cancelledJobs.contains(jobId)) break
                if (readLen > 0) {
                     val chunk = String(byteBuffer, 0, readLen, Charsets.UTF_8)
                     for (c in chunk) {
                        if (c == '\r' || c == '\n') {
                            if (sb.isNotEmpty()) {
                                val line = sb.toString()
                                sb.setLength(0)

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
                                    
                                    if (percent > 0) {
                                        onProgress?.invoke(percent, currentSpeed)
                                    }
                                }
                            }
                        } else {
                            sb.append(c)
                        }
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
        NativeLibPaths.init(context)
        
        val ffmpegPath = File(NativeLibPaths.binDir, "libffmpeg.so").absolutePath
        
        return try {
            val processBuilder = ProcessBuilder(listOf(ffmpegPath, "-i", filePath))
                .redirectErrorStream(true)

            processBuilder.environment().apply {
                this["LD_LIBRARY_PATH"] = NativeLibPaths.ldLibraryPath
                this["PATH"] = System.getenv("PATH") + ":" + NativeLibPaths.binDir
            }
            
            val process = processBuilder.start()
            process.outputStream.close()
            
            var duration: Double? = null
            val reader = process.inputStream.bufferedReader()
            val buffer = CharArray(1024)
            val sb = StringBuilder()
            var read: Int
            while (reader.read(buffer).also { read = it } != -1) {
                for (i in 0 until read) {
                    val c = buffer[i]
                    if (c == '\r' || c == '\n') {
                        if (sb.isNotEmpty()) {
                            val line = sb.toString()
                            sb.setLength(0)
                            if (line.contains("Duration:")) {
                                val durationMatch = Regex("""Duration:\s*(\d+):(\d+):(\d+(?:\.\d+)?)""").find(line)
                                if (durationMatch != null) {
                                    val (hours, minutes, seconds) = durationMatch.destructured
                                    duration = hours.toDouble() * 3600 + minutes.toDouble() * 60 + seconds.toDouble()
                                }
                            }
                        }
                    } else {
                        sb.append(c)
                    }
                }
            }
            process.waitFor()
            duration
        } catch (e: Exception) {
            Log.e(TAG, "Failed to get duration", e)
            null
        }
    }

    sealed class ExecuteResult {
        object Success : ExecuteResult()
        data class Failed(val error: String) : ExecuteResult()
        object Cancelled : ExecuteResult()
    }
}
