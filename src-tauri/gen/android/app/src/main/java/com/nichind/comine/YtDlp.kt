package com.nichind.comine

import android.app.Application
import android.os.Environment
import android.util.Log
import com.yausername.aria2c.Aria2c
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import org.json.JSONObject
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.BufferedInputStream
import java.io.FileOutputStream
import java.io.File
import java.net.URL
import java.util.concurrent.ConcurrentHashMap

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
            onReady?.invoke()
        }.start()
    }

    fun getVersion(app: Application): String = runCatching { YoutubeDL.getInstance().versionName(app) ?: "" }.getOrDefault("")

    fun cancel(jobId: String): Boolean {
        // Cancel both yt-dlp and any follow-up ffmpeg embedding jobs.
        runCatching { FFmpegExecutor.cancel("$jobId:thumb") }
        runCatching { FFmpegExecutor.cancel("$jobId:thumbimg") }
        runCatching { cleanupProgressTracker(jobId) }
        return runCatching { YoutubeDL.getInstance().destroyProcessById(jobId) }.getOrDefault(false)
    }

    data class ProgressUpdate(
        val percent: Int,
        val speedBps: Long? = null,
        val etaSeconds: Long? = null,
        val downloadedBytes: Long = 0,
        val totalBytes: Long? = null,
        val filename: String? = null,
    )

    // yt-dlp may download multiple files (video/audio/merge); track filename to keep progress monotonic.
    private data class MultiPhaseProgress(
        var completedBytes: Long = 0,
        var completedTotal: Long = 0,
        var currentFilename: String? = null,
        var currentFileTotal: Long? = null,
        var currentPhaseMaxDownloaded: Long = 0,
    )

    private val progressTrackers = ConcurrentHashMap<String, MultiPhaseProgress>()

    private fun cleanupProgressTracker(jobId: String) {
        progressTrackers.remove(jobId)
    }

    private fun updateCumulative(jobId: String, downloaded: Long, total: Long?, filename: String?): Pair<Long, Long?> {
        val tracker = progressTrackers.getOrPut(jobId) { MultiPhaseProgress() }

        val filenameChanged = tracker.currentFilename != null && filename != null && tracker.currentFilename != filename
        if (filenameChanged) {
            val prevTotal = tracker.currentFileTotal
            if (prevTotal != null && prevTotal > 0) {
                tracker.completedBytes += prevTotal
                tracker.completedTotal += prevTotal
            } else {
                tracker.completedBytes += tracker.currentPhaseMaxDownloaded
                tracker.completedTotal += tracker.currentPhaseMaxDownloaded
            }
            tracker.currentPhaseMaxDownloaded = 0
            tracker.currentFileTotal = null
        }

        if (!filename.isNullOrBlank()) {
            tracker.currentFilename = filename
        }

        if (total != null && total > 0) {
            tracker.currentFileTotal = total
        }

        tracker.currentPhaseMaxDownloaded = maxOf(tracker.currentPhaseMaxDownloaded, downloaded)

        val cumulativeDownloaded = tracker.completedBytes + downloaded
        val cumulativeTotal = tracker.currentFileTotal?.let { tracker.completedTotal + it }
        return cumulativeDownloaded to cumulativeTotal
    }

    private fun parseProgressLine(line: String, jobId: String): ProgressUpdate? {
        val start = line.indexOf("__COMINE_PROGRESS__")
        val end = line.lastIndexOf("__COMINE_PROGRESS__")
        if (start < 0 || end < 0 || start >= end) return null

        val content = line.substring(start + 19, end)

        fun extractValue(key: String): String? {
            val pattern = "$key:"
            val idx = content.indexOf(pattern)
            if (idx < 0) return null
            val valueStart = idx + pattern.length
            val rest = content.substring(valueStart)
            val endIdx = rest.indexOfFirst { it == ',' || it == '}' }.let { if (it < 0) rest.length else it }
            return rest.substring(0, endIdx).trim().takeIf { it.isNotBlank() }
        }

        fun parseU64(s: String?): Long? {
            val v = s?.trim() ?: return null
            if (v.isEmpty() || v == "NA") return null
            return v.toDoubleOrNull()?.toLong()
        }

        val downloaded = parseU64(extractValue("downloaded")) ?: 0L
        val total = parseU64(extractValue("total")) ?: parseU64(extractValue("total_estimate"))
        val speed = parseU64(extractValue("speed"))
        val eta = parseU64(extractValue("eta"))
        val filename = extractValue("filename")?.takeIf { it != "NA" && it.isNotBlank() }

        val (cumulativeDownloaded, cumulativeTotal) = updateCumulative(jobId, downloaded, total, filename)

        val percent = if (cumulativeTotal != null && cumulativeTotal > 0) {
            ((cumulativeDownloaded.toDouble() / cumulativeTotal.toDouble()) * 100.0)
                .coerceIn(0.0, 100.0)
                .toInt()
        } else {
            0
        }

        return ProgressUpdate(
            percent = percent,
            speedBps = speed,
            etaSeconds = eta,
            downloadedBytes = cumulativeDownloaded,
            totalBytes = cumulativeTotal,
            filename = filename
        )
    }

    // Returns NDJSON (one JSON object per line) for playlists/channels.
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
                    addOption("--extractor-args", "youtube:player_client=$youtubePlayerClient;player_skip=webpage,configs")
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

    private fun JSONObject.safeString(key: String): String? {
        if (isNull(key)) return null
        val value = optString(key, "")
        return if (value.isBlank() || value == "null") null else value
    }

    private fun JSONObject.safeStringAny(vararg keys: String): String? {
        for (k in keys) {
            safeString(k)?.let { return it }
        }
        return null
    }

    private fun JSONObject.optBooleanAny(vararg keys: String, default: Boolean = false): Boolean {
        for (k in keys) {
            if (has(k) && !isNull(k)) return optBoolean(k, default)
        }
        return default
    }

    private fun JSONObject.optIntAny(vararg keys: String): Int? {
        for (k in keys) {
            if (has(k) && !isNull(k)) return optInt(k)
        }
        return null
    }

    private fun JSONObject.optLongAny(vararg keys: String): Long? {
        for (k in keys) {
            if (has(k) && !isNull(k)) return optLong(k)
        }
        return null
    }

    fun execute(
        jobId: String,
        requestJson: String,
        onProgress: ((ProgressUpdate) -> Unit)? = null,
        onStage: ((String) -> Unit)? = null,
        onTitle: ((String) -> Unit)? = null,
    ): ExecuteResult {
        if (!initialized) return ExecuteResult.Failed("yt-dlp not initialized")

        return try {
            val root = JSONObject(requestJson)
            val url = root.safeString("url") ?: return ExecuteResult.Failed("missing_url")

            val output = root.optJSONObject("output")
            val outputDir = output?.safeStringAny("directory")
            val filenameTemplate = output?.safeStringAny("filenameTemplate", "filename_template") ?: "%(title)s.%(ext)s"

            val quality = root.optJSONObject("quality")
            // Rust/TS bindings serialize as camelCase, but accept snake_case for compatibility.
            val audioOnly = quality?.optBooleanAny("audioOnly", "audio_only", default = false) ?: false
            val format = quality?.safeStringAny("format") ?: "best"
            val maxHeight = quality?.optIntAny("maxHeight", "max_height")?.takeIf { it > 0 }
            val audioFormat = quality?.safeStringAny("audioFormat", "audio_format")

            val opts = root.optJSONObject("options")
            val embedThumbnail = opts?.optBooleanAny("embedThumbnail", "embed_thumbnail", default = true) ?: true
            val embedMetadata = opts?.optBooleanAny("embedMetadata", "embed_metadata", default = true) ?: true
            val embedSubtitles = opts?.optBooleanAny("embedSubtitles", "embed_subtitles", default = false) ?: false
            val subtitleLangs = opts?.safeStringAny("subtitleLangs", "subtitle_langs")
            val sponsorblockRemove = opts?.safeStringAny("sponsorblockRemove", "sponsorblock_remove")
            val youtubePlayerClient = opts?.safeStringAny("youtubePlayerClient", "youtube_player_client")
            val aria2Connections = (opts?.optIntAny("aria2Connections", "aria2_connections") ?: 8).coerceIn(1, 16)
            val aria2Splits = (opts?.optIntAny("aria2Splits", "aria2_splits") ?: 8).coerceIn(1, 16)
            val speedLimitBytes = opts?.optLongAny("speedLimit", "speed_limit")?.takeIf { it > 0L }
            val proxyObj = opts?.optJSONObject("proxy")
            val proxyUrl = if (proxyObj?.optBoolean("enabled", false) == true) proxyObj.safeString("url") else null
            val cookiePath = opts?.safeStringAny("customCookies", "custom_cookies")

            val dlDir = if (!outputDir.isNullOrBlank()) File(outputDir) else File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Comine")
            if (!dlDir.exists()) dlDir.mkdirs()

            val scanStartMs = System.currentTimeMillis()
            var artifactPath: String? = null
            var capturedThumbnailUrl: String? = null
            var capturedTitle: String? = null

            fun parsePrinted(prefix: String, line: String): String? {
                val t = line.trim()
                if (!t.startsWith(prefix)) return null
                return t.removePrefix(prefix).trim().trim('"').ifBlank { null }
            }

            val request = YoutubeDLRequest(url).apply {
                addOption("-o", "${dlDir.absolutePath}/$filenameTemplate")
                addOption("--encoding", "utf-8")
                addOption("--continue")
                addOption("--newline")
                addOption("--progress")
                addOption("--progress-delta", "0.5")

                // Match desktop progress events (we parse __COMINE_PROGRESS__ markers below).
                addOption(
                    "--progress-template",
                    "download:__COMINE_PROGRESS__{downloaded:%(progress.downloaded_bytes)s,total:%(progress.total_bytes)s,total_estimate:%(progress.total_bytes_estimate)s,speed:%(progress.speed)s,eta:%(progress.eta)s,filename:%(progress.filename)s}__COMINE_PROGRESS__"
                )

                // Print metadata early so we can embed thumbnails post-download (audio-only)
                addOption("--print", "pre_process:>>>TITLE:%(title)s")
                addOption("--print", "pre_process:>>>THUMBNAIL:%(thumbnail)s")
                addOption("--print", "after_move:>>>FILEPATH:%(filepath)s")

                // Always pass format filter - desktop does this too
                addOption("-f", format)

                if (audioOnly) {
                    addOption("-x")
                    audioFormat?.let { addOption("--audio-format", it) }
                } else {
                    maxHeight?.let { addOption("-S", "res:$it") }
                }

                youtubePlayerClient?.let { addOption("--extractor-args", "youtube:player_client=$it;player_skip=webpage,configs") }
                proxyUrl?.let { addOption("--proxy", it) }
                cookiePath?.let { addOption("--cookies", it) }

                if (ffmpegAvailable) {
                    // Match desktop behavior:
                    // - non-audio: let yt-dlp embed thumbnail
                    // - audio-only: embed ourselves after download
                    if (embedThumbnail && !audioOnly) {
                        addOption("--embed-thumbnail")
                        addOption("--convert-thumbnails", "jpg")
                    }
                    if (embedMetadata) addOption("--embed-metadata")
                    if (embedSubtitles) {
                        addOption("--write-subs"); addOption("--write-auto-subs")
                        subtitleLangs?.let { addOption("--sub-langs", it) }
                        addOption("--embed-subs")
                    }
                }

                sponsorblockRemove?.let { addOption("--sponsorblock-remove", it) }

                if (aria2Available) {
                    addOption("--downloader", "libaria2c.so")
                    addOption("--external-downloader-args", "aria2c:'-x $aria2Connections -s $aria2Splits -k 1M'")
                }

                speedLimitBytes?.let { addOption("--limit-rate", "${(it / 1024).coerceAtLeast(1)}K") }
            }

            onStage?.invoke("Downloading")

            // Helper to process each progress line - avoids duplicating logic in both execute() branches
            fun handleProgressLine(p: Float?, eta: Long?, line: String?) {
                val parsed0 = line?.let { parseProgressLine(it, jobId) }
                val parsed = if (parsed0 != null && (parsed0.totalBytes == null || parsed0.totalBytes == 0L) && (p ?: 0f) > 0f) {
                    parsed0.copy(percent = (p ?: 0f).toInt())
                } else {
                    parsed0
                }
                if (parsed != null) {
                    onProgress?.invoke(parsed)
                    RustBridge.notifyProgress(
                        jobId,
                        parsed.percent.toFloat(),
                        parsed.speedBps ?: 0L,
                        parsed.etaSeconds ?: -1L,
                        parsed.downloadedBytes,
                        parsed.totalBytes ?: 0L
                    )
                } else {
                    val pct = p?.toInt() ?: 0
                    val fallback = ProgressUpdate(percent = pct, etaSeconds = eta?.toLong())
                    onProgress?.invoke(fallback)
                    RustBridge.notifyProgress(jobId, p ?: 0f, 0L, eta?.toLong() ?: -1L, 0L, 0L)
                }
                line?.let {
                    extractOutputPath(it)?.let { path -> artifactPath = path }
                    parsePrinted(">>>THUMBNAIL:", it)?.let { t -> capturedThumbnailUrl = t }
                    parsePrinted(">>>TITLE:", it)?.let { t ->
                        capturedTitle = t
                        onTitle?.invoke(t)
                    }
                    parsePrinted(">>>FILEPATH:", it)?.let { path -> artifactPath = path }
                }
            }

            val response = runCatching {
                YoutubeDL.getInstance().execute(request, jobId) { p, eta, line ->
                    handleProgressLine(p, eta, line)
                }
            }.getOrElse {
                YoutubeDL.getInstance().execute(request) { p, eta, line ->
                    handleProgressLine(p, eta, line)
                }
            }

            var outputPath = artifactPath ?: guessOutputPath("${response.out}\n${response.err}") ?: scanLatestFile(dlDir, scanStartMs)

            val title = capturedTitle
                ?: outputPath?.let { path ->
                    File(path).nameWithoutExtension
                        .takeIf { it.isNotBlank() && it != "%(title)s" }
                }

            // Desktop embeds thumbnails for audio-only as a separate step.
            // Do the same here using FFmpegExecutor.
            if (
                response.exitCode == 0 &&
                audioOnly &&
                embedThumbnail &&
                ffmpegAvailable &&
                !outputPath.isNullOrBlank()
            ) {
                val ctx = app
                if (ctx != null) {
                    val thumbUrl = capturedThumbnailUrl
                        ?: runCatching {
                            // Fallback: resolve thumbnail if it wasn't captured from --print
                            val resolved = resolve(url, flatPlaylist = false, youtubePlayerClient = youtubePlayerClient)
                            when (resolved) {
                                is ResolveResult.Success -> {
                                    JSONObject(resolved.output).optString("thumbnail", "").takeIf { it.isNotBlank() }
                                }
                                else -> null
                            }
                        }.getOrNull()

                    if (!thumbUrl.isNullOrBlank()) {
                        onStage?.invoke("Embedding cover art")
                        outputPath = runCatching {
                            embedThumbnailWithFfmpeg(ctx, jobId, outputPath!!, thumbUrl)
                        }.onFailure {
                            Log.w(TAG, "Thumbnail embedding failed: ${it.message}")
                        }.getOrDefault(outputPath)
                    }
                }
            }

            cleanupProgressTracker(jobId)

            if (response.exitCode == 0) ExecuteResult.Success(outputPath ?: dlDir.absolutePath, title, capturedThumbnailUrl)
            else ExecuteResult.Failed(response.err ?: "exitCode=${response.exitCode}")
        } catch (e: Exception) {
            cleanupProgressTracker(jobId)
            ExecuteResult.Failed(e.message ?: "unknown")
        }
    }

    private fun embedThumbnailWithFfmpeg(
        app: Application,
        jobId: String,
        audioPath: String,
        thumbnailUrl: String,
    ): String {
        val inputAudio = File(audioPath)
        if (!inputAudio.exists()) return audioPath

        val ext = inputAudio.extension.lowercase()
        val outputExt = if (ext == "opus") "ogg" else ext

        val cacheDir = File(app.cacheDir, "thumb").apply { mkdirs() }
        val thumbSrc = File(cacheDir, "${jobId}_src")
        val thumbJpg = File(cacheDir, "${jobId}_cover.jpg")

        downloadToFile(thumbnailUrl, thumbSrc)

        val letterboxed = runCatching {
            val bmp = BitmapFactory.decodeFile(thumbSrc.absolutePath) ?: return@runCatching false
            try {
                isLetterboxedThumbnail(bmp)
            } finally {
                bmp.recycle()
            }
        }.getOrDefault(false)

        // Convert + square-pad thumbnail to JPEG for broad container compatibility.
        val vf = if (letterboxed) {
            // Desktop behavior: crop letterboxed (side bars) to center square.
            "crop=ih:ih:(iw-ih)/2:0,scale=600:600,format=yuvj420p"
        } else {
            // Preserve non-letterboxed art: fit inside square with padding.
            "scale=600:600:force_original_aspect_ratio=decrease,pad=600:600:(ow-iw)/2:(oh-ih)/2,format=yuvj420p"
        }

        val imgArgs = listOf(
            "-y",
            "-i",
            thumbSrc.absolutePath,
            "-vf",
            vf,
            "-q:v",
            "2",
            thumbJpg.absolutePath,
        )
        when (val r = FFmpegExecutor.execute(app, "$jobId:thumbimg", imgArgs, null, null)) {
            is FFmpegExecutor.ExecuteResult.Success -> {}
            is FFmpegExecutor.ExecuteResult.Cancelled -> return audioPath
            is FFmpegExecutor.ExecuteResult.Failed -> throw RuntimeException(r.error)
        }

        val tempOut = File(inputAudio.parentFile, "${inputAudio.nameWithoutExtension}.temp.$outputExt")
        val finalOut = if (ext == "opus") File(inputAudio.parentFile, "${inputAudio.nameWithoutExtension}.ogg") else inputAudio

        val embedArgs = buildList {
            add("-y")
            add("-i"); add(inputAudio.absolutePath)
            add("-i"); add(thumbJpg.absolutePath)
            add("-map"); add("0")
            add("-map"); add("1")
            add("-c"); add("copy")
            // Ensure cover art is treated as attached picture
            add("-disposition:v:0"); add("attached_pic")
            add("-metadata:s:v"); add("title=Album cover")
            add("-metadata:s:v"); add("comment=Cover (front)")

            if (outputExt == "mp3") {
                add("-id3v2_version"); add("3")
                add("-write_id3v1"); add("1")
            }

            add(tempOut.absolutePath)
        }

        when (val r = FFmpegExecutor.execute(app, "$jobId:thumb", embedArgs, null, null)) {
            is FFmpegExecutor.ExecuteResult.Success -> {}
            is FFmpegExecutor.ExecuteResult.Cancelled -> return audioPath
            is FFmpegExecutor.ExecuteResult.Failed -> throw RuntimeException(r.error)
        }

        // Replace original file (or write new extension for opus->ogg)
        if (finalOut.absolutePath == inputAudio.absolutePath) {
            val bak = File(inputAudio.parentFile, "${inputAudio.name}.bak")
            if (!inputAudio.renameTo(bak)) {
                tempOut.delete()
                return audioPath
            }
            if (!tempOut.renameTo(finalOut)) {
                // rollback
                bak.renameTo(inputAudio)
                tempOut.delete()
                return audioPath
            }
            bak.delete()
        } else {
            // opus -> ogg: keep original if rename fails, but prefer the new file.
            if (!tempOut.renameTo(finalOut)) {
                tempOut.delete()
                return audioPath
            }
        }

        // Best-effort cleanup
        runCatching { thumbSrc.delete() }
        runCatching { thumbJpg.delete() }

        return finalOut.absolutePath
    }

    private fun downloadToFile(url: String, dest: File) {
        dest.parentFile?.mkdirs()
        URL(url).openConnection().apply {
            connectTimeout = 15000
            readTimeout = 30000
        }.getInputStream().use { input ->
            BufferedInputStream(input).use { buffered ->
                FileOutputStream(dest).use { out ->
                    buffered.copyTo(out)
                }
            }
        }
    }

    private fun isLetterboxedThumbnail(bitmap: Bitmap): Boolean {
        val width = bitmap.width
        val height = bitmap.height

        if (width <= height) return false

        val squareSize = height
        val barWidth = (width - squareSize) / 2

        if (barWidth < (width / 20)) return false

        val darkThreshold = 30
        val tolerance = 60

        fun rgbAt(x: Int, y: Int): IntArray {
            val p = bitmap.getPixel(x.coerceIn(0, width - 1), y.coerceIn(0, height - 1))
            return intArrayOf(
                (p shr 16) and 0xFF,
                (p shr 8) and 0xFF,
                p and 0xFF
            )
        }

        val samplePointsLeft = arrayOf(
            Pair(barWidth / 4, height / 4),
            Pair(barWidth / 4, height / 2),
            Pair(barWidth / 4, height * 3 / 4),
            Pair(barWidth / 2, height / 4),
            Pair(barWidth / 2, height / 2),
            Pair(barWidth / 2, height * 3 / 4),
            Pair(barWidth * 3 / 4, height / 4),
            Pair(barWidth * 3 / 4, height / 2),
            Pair(barWidth * 3 / 4, height * 3 / 4),
        )
        val samplePointsRight = arrayOf(
            Pair(width - barWidth / 4, height / 4),
            Pair(width - barWidth / 4, height / 2),
            Pair(width - barWidth / 4, height * 3 / 4),
            Pair(width - barWidth / 2, height / 4),
            Pair(width - barWidth / 2, height / 2),
            Pair(width - barWidth / 2, height * 3 / 4),
            Pair(width - barWidth * 3 / 4, height / 4),
            Pair(width - barWidth * 3 / 4, height / 2),
            Pair(width - barWidth * 3 / 4, height * 3 / 4),
        )

        val all = samplePointsLeft.asList() + samplePointsRight.asList()
        val totalSamples = all.size

        var darkCount = 0
        for ((x, y) in all) {
            val (r, g, b) = rgbAt(x, y)
            if (r <= darkThreshold && g <= darkThreshold && b <= darkThreshold) {
                darkCount += 1
            }
        }
        val requiredDark = (totalSamples * 7) / 10
        if (darkCount >= requiredDark) return true

        val ref = rgbAt(barWidth / 2, height / 2)
        var uniformCount = 0
        for ((x, y) in all) {
            val c = rgbAt(x, y)
            val dr = kotlin.math.abs(c[0] - ref[0])
            val dg = kotlin.math.abs(c[1] - ref[1])
            val db = kotlin.math.abs(c[2] - ref[2])
            if (dr <= tolerance && dg <= tolerance && db <= tolerance) {
                uniformCount += 1
            }
        }
        val requiredUniform = (totalSamples * 7) / 10
        return uniformCount >= requiredUniform
    }

    sealed class ExecuteResult {
        data class Success(val outputPath: String, val title: String? = null, val thumbnailUrl: String? = null) : ExecuteResult()
        data class Failed(val error: String) : ExecuteResult()
    }

    private fun guessOutputPath(output: String): String? {
        val lines = output.lines().asReversed()
        fun match(prefix: String) = lines.firstOrNull { it.contains(prefix) }?.substringAfter(prefix)?.trim()?.trim('"')
        return match("Destination:")
            ?: Regex("""\[Merger] Merging formats into "(.+?)"""").find(output)?.groupValues?.get(1)
            ?: Regex("""\[ffmpeg] Destination: (.+)""").findAll(output).lastOrNull()?.groupValues?.get(1)
    }

    private fun extractOutputPath(line: String): String? {
        val t = line.trim()
        if (t.isEmpty()) return null
        fun after(prefix: String) = if (t.contains(prefix)) t.substringAfter(prefix).trim().trim('"').ifBlank { null } else null
        return after("Destination:")
            ?: Regex("""\[Merger] Merging formats into "(.+?)"""").find(t)?.groupValues?.get(1)
            ?: Regex("""\[ffmpeg] Destination: (.+)""").find(t)?.groupValues?.get(1)
            ?: Regex("""\[ExtractAudio] Destination: (.+)""").find(t)?.groupValues?.get(1)
    }

    private fun scanLatestFile(dir: File, sinceMs: Long): String? = runCatching {
        dir.listFiles()?.asSequence()
            ?.filter { it.isFile && !it.name.lowercase().let { n -> n.endsWith(".part") || n.endsWith(".tmp") || n.endsWith(".ytdl") } }
            ?.filter { it.lastModified() >= sinceMs - 5000 }
            ?.maxByOrNull { it.lastModified() }?.absolutePath
    }.getOrNull()
}