package com.nichind.comine

import android.app.Application
import android.os.Environment
import android.util.Log
import com.yausername.aria2c.Aria2c
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import org.json.JSONObject
import java.io.File

object YtDlp {
    private const val TAG = "YtDlp"

    @Volatile var initialized = false; private set
    @Volatile var ffmpegAvailable = false; private set
    @Volatile var aria2Available = false; private set

    fun init(app: Application, onReady: (() -> Unit)? = null) {
        Thread {
            runCatching { YoutubeDL.getInstance().init(app); initialized = true; Log.i(TAG, "yt-dlp ready") }
            runCatching { FFmpeg.getInstance().init(app); ffmpegAvailable = true; Log.i(TAG, "ffmpeg ready") }
            runCatching { Aria2c.getInstance().init(app); aria2Available = true; Log.i(TAG, "aria2 ready") }
            onReady?.invoke()
        }.start()
    }

    fun getVersion(app: Application): String = runCatching { YoutubeDL.getInstance().versionName(app) ?: "" }.getOrDefault("")

    fun cancel(jobId: String): Boolean = runCatching { YoutubeDL.getInstance().destroyProcessById(jobId) }.getOrDefault(false)

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

    fun execute(jobId: String, requestJson: String, onProgress: ((Int, String) -> Unit)? = null): ExecuteResult {
        if (!initialized) return ExecuteResult.Failed("yt-dlp not initialized")

        return try {
            val root = JSONObject(requestJson)
            val url = root.safeString("url") ?: return ExecuteResult.Failed("missing_url")

            val output = root.optJSONObject("output")
            val outputDir = output?.safeString("directory")
            val filenameTemplate = output?.safeString("filename_template") ?: "%(title)s.%(ext)s"

            val quality = root.optJSONObject("quality")
            val audioOnly = quality?.optBoolean("audio_only", false) ?: false
            val format = quality?.safeString("format") ?: "best"
            val maxHeight = quality?.optInt("max_height", 0)?.takeIf { it > 0 }
            val audioFormat = quality?.safeString("audio_format")

            val opts = root.optJSONObject("options")
            val embedThumbnail = opts?.optBoolean("embed_thumbnail", true) ?: true
            val embedMetadata = opts?.optBoolean("embed_metadata", true) ?: true
            val embedSubtitles = opts?.optBoolean("embed_subtitles", false) ?: false
            val subtitleLangs = opts?.safeString("subtitle_langs")
            val sponsorblockRemove = opts?.safeString("sponsorblock_remove")
            val youtubePlayerClient = opts?.safeString("youtube_player_client")
            val aria2Connections = opts?.optInt("aria2_connections", 8)?.coerceIn(1, 16) ?: 8
            val aria2Splits = opts?.optInt("aria2_splits", 8)?.coerceIn(1, 16) ?: 8
            val speedLimitBytes = opts?.optLong("speed_limit", 0L)?.takeIf { it > 0L }
            val proxyObj = opts?.optJSONObject("proxy")
            val proxyUrl = if (proxyObj?.optBoolean("enabled", false) == true) proxyObj.safeString("url") else null
            val cookiePath = opts?.safeString("custom_cookies")

            val dlDir = if (!outputDir.isNullOrBlank()) File(outputDir) else File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Comine")
            if (!dlDir.exists()) dlDir.mkdirs()

            val scanStartMs = System.currentTimeMillis()
            var artifactPath: String? = null

            val request = YoutubeDLRequest(url).apply {
                addOption("-o", "${dlDir.absolutePath}/$filenameTemplate")
                addOption("--encoding", "utf-8")
                addOption("--continue")
                addOption("--progress-delta", "0.25")

                if (!audioOnly) {
                    addOption("-f", format)
                    maxHeight?.let { addOption("-S", "res:$it") }
                } else {
                    addOption("-x")
                    audioFormat?.let { addOption("--audio-format", it) }
                }

                youtubePlayerClient?.let { addOption("--extractor-args", "youtube:player_client=$it;player_skip=webpage,configs") }
                proxyUrl?.let { addOption("--proxy", it) }
                cookiePath?.let { addOption("--cookies", it) }

                if (ffmpegAvailable) {
                    if (embedThumbnail) { addOption("--embed-thumbnail"); addOption("--convert-thumbnails", "jpg") }
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

            val response = runCatching {
                YoutubeDL.getInstance().execute(request, jobId) { p, eta, line ->
                    onProgress?.invoke(p?.toInt() ?: 0, eta?.toString() ?: "")
                    RustBridge.notifyProgress(jobId, p ?: 0f, 0L, eta?.toLong() ?: -1L, 0L, 0L)
                    line?.let { extractOutputPath(it)?.let { path -> artifactPath = path } }
                }
            }.getOrElse {
                YoutubeDL.getInstance().execute(request) { p, eta, line ->
                    onProgress?.invoke(p?.toInt() ?: 0, eta?.toString() ?: "")
                    RustBridge.notifyProgress(jobId, p ?: 0f, 0L, eta?.toLong() ?: -1L, 0L, 0L)
                    line?.let { extractOutputPath(it)?.let { path -> artifactPath = path } }
                }
            }

            val outputPath = artifactPath ?: guessOutputPath("${response.out}\n${response.err}") ?: scanLatestFile(dlDir, scanStartMs)

            val title = outputPath?.let { path ->
                File(path).nameWithoutExtension
                    .takeIf { it.isNotBlank() && it != "%(title)s" }
            }

            if (response.exitCode == 0) ExecuteResult.Success(outputPath ?: dlDir.absolutePath, title)
            else ExecuteResult.Failed(response.err ?: "exitCode=${response.exitCode}")
        } catch (e: Exception) {
            ExecuteResult.Failed(e.message ?: "unknown")
        }
    }

    sealed class ExecuteResult {
        data class Success(val outputPath: String, val title: String? = null) : ExecuteResult()
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