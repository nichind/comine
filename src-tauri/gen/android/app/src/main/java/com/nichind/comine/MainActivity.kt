package com.nichind.comine

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
import android.os.Build
import android.content.Context
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.util.Base64
import android.util.Log
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import com.yausername.aria2c.Aria2c
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection

import java.net.URL
import java.net.URLConnection
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

class MainActivity : TauriActivity() {
  companion object {
    private const val TAG = "Comine"
    private const val MAX_CONCURRENT_DOWNLOADS = 3
  }

  private var webView: WebView? = null
  private val mainHandler = Handler(Looper.getMainLooper())

  private val downloadExecutor = Executors.newFixedThreadPool(MAX_CONCURRENT_DOWNLOADS)
  private val infoExecutor = Executors.newCachedThreadPool()

  @Volatile private var ytdlInitialized: Boolean = false
  @Volatile private var ffmpegAvailable: Boolean = false
  @Volatile private var aria2Available: Boolean = false

  private var pendingShareUrl: String? = null

  private var folderPickerCallback: String? = null
  private var filePickerCallback: String? = null

  private lateinit var folderPickerLauncher: ActivityResultLauncher<Uri?>
  private lateinit var filePickerLauncher: ActivityResultLauncher<Array<String>>

  private val jobIdToFfmpegProcess = ConcurrentHashMap<String, Process>()

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()

    folderPickerLauncher = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
      val callbackName = folderPickerCallback
      folderPickerCallback = null

      if (callbackName == null) return@registerForActivityResult

      val resultJson = if (uri != null) {
        try {
          contentResolver.takePersistableUriPermission(
            uri,
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
          )
        } catch (_: Exception) {
        }

        JSONObject().apply {
          put("success", true)
          put("uri", uri.toString())
          put("path", uri.toString())
        }.toString()
      } else {
        JSONObject().apply {
          put("success", false)
          put("cancelled", true)
        }.toString()
      }

      sendCallback(callbackName, resultJson)
    }

    filePickerLauncher = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
      val callbackName = filePickerCallback
      filePickerCallback = null
      if (callbackName == null) return@registerForActivityResult

      if (uri != null) {
        try {
          contentResolver.takePersistableUriPermission(
            uri,
            Intent.FLAG_GRANT_READ_URI_PERMISSION
          )
        } catch (_: Exception) {
        }
        sendCallback(callbackName, uri.toString())
      } else {
        sendCallbackNull(callbackName)
      }
    }

    super.onCreate(savedInstanceState)
    initYoutubeDlInBackground()
    handleIntent(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    handleIntent(intent)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    this.webView = webView
    webView.addJavascriptInterface(AndroidYtDlpBridge(), "AndroidYtDlp")
    webView.addJavascriptInterface(AndroidColorsBridge(this@MainActivity), "AndroidColors")

    if (ytdlInitialized) {
      dispatchYtdlpReady()
    }

    val url = pendingShareUrl
    if (!url.isNullOrBlank()) {
      pendingShareUrl = null
      dispatchShareIntent(url)
    }
  }

  override fun onDestroy() {
    super.onDestroy()
    try {
      downloadExecutor.shutdownNow()
    } catch (_: Exception) {
    }
    try {
      infoExecutor.shutdownNow()
    } catch (_: Exception) {
    }
  }

  private fun initYoutubeDlInBackground() {
    infoExecutor.execute {
      try {
        YoutubeDL.getInstance().init(application)
        ytdlInitialized = true
        sendLog("info", "yt-dlp initialized")
        dispatchYtdlpReady()
      } catch (e: Exception) {
        ytdlInitialized = false
        sendLog("error", "yt-dlp init failed: ${e.message}")
      }

      try {
        FFmpeg.getInstance().init(application)
        ffmpegAvailable = true
        sendLog("info", "ffmpeg initialized")
      } catch (e: Exception) {
        ffmpegAvailable = false
        sendLog("warn", "ffmpeg init failed: ${e.message}")
      }

      try {
        Aria2c.getInstance().init(application)
        aria2Available = true
        sendLog("info", "aria2 initialized")
      } catch (e: Exception) {
        aria2Available = false
        sendLog("warn", "aria2 init failed: ${e.message}")
      }

      dispatchYtdlpReady()
    }
  }

  private fun handleIntent(intent: Intent?) {
    if (intent == null) return

    val url: String? = when (intent.action) {
      Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)
      Intent.ACTION_VIEW -> intent.dataString
      else -> null
    }

    if (url.isNullOrBlank()) return
    if (webView == null) {
      pendingShareUrl = url
      return
    }
    dispatchShareIntent(url)
  }

  private fun dispatchShareIntent(url: String) {
    val urlLit = JSONObject.quote(url)
    evalJs(
      """
      (function() {
        try {
          window.dispatchEvent(new CustomEvent('share-intent', { detail: { url: $urlLit } }));
        } catch (e) {}
      })();
      """.trimIndent()
    )
  }

  private fun dispatchYtdlpReady() {
    evalJs(
      """
      (function() {
        try {
          window.__YTDLP_READY__ = true;
          window.dispatchEvent(new Event('ytdlp-ready'));
        } catch (e) {}
      })();
      """.trimIndent()
    )
  }

  private fun evalJs(js: String) {
    mainHandler.post {
      try {
        webView?.evaluateJavascript(js, null)
      } catch (_: Exception) {
      }
    }
  }

  private fun sendCallback(callbackName: String, arg: String) {
    val cb = JSONObject.quote(callbackName)
    val argLit = JSONObject.quote(arg)
    evalJs(
      """
      (function() {
        try {
          var fn = window[$cb];
          if (typeof fn === 'function') fn($argLit);
        } catch (e) {}
      })();
      """.trimIndent()
    )
  }

  private fun sendCallbackNull(callbackName: String) {
    val cb = JSONObject.quote(callbackName)
    evalJs(
      """
      (function() {
        try {
          var fn = window[$cb];
          if (typeof fn === 'function') fn(null);
        } catch (e) {}
      })();
      """.trimIndent()
    )
  }

  private fun sendJobEvent(json: JSONObject) {
    val payload = JSONObject.quote(json.toString())
    evalJs(
      """
      (function() {
        try {
          window.dispatchEvent(new CustomEvent('job-event', { detail: JSON.parse($payload) }));
        } catch (e) {
          try {
            if (window.__androidLog) {
              var msg = '' + e;
              window.__androidLog('error', 'Android', 'Failed to dispatch job-event: ' + msg);
            }
          } catch (e2) {}
        }
      })();
      """.trimIndent()
    )
  }

  private fun sendLog(level: String, message: String) {
    Log.d(TAG, "[$level] $message")
    val lvl = JSONObject.quote(level)
    val msg = JSONObject.quote(message)
    evalJs(
      """
      (function() {
        try {
          if (window.__androidLog) window.__androidLog($lvl, 'Android', $msg);
        } catch (e) {}
      })();
      """.trimIndent()
    )
  }

  private fun safePlaylistFolderName(input: String): String {
    return input
      .replace(Regex("[<>:\\\"/\\\\|?*]"), "_")
      .replace(Regex("\\s+"), " ")
      .trim()
      .take(100)
  }

  private fun bestGuessOutputPath(output: String): String? {
    val lines = output.lines().asReversed()

    fun match(prefix: String): String? {
      val hit = lines.firstOrNull { it.contains(prefix) } ?: return null
      return hit.substringAfter(prefix).trim().trim('"')
    }

    return match("Destination:")
      ?: Regex("""\[Merger\] Merging formats into \"(.+?)\"""").find(output)?.groupValues?.get(1)
      ?: Regex("""\[ffmpeg\] Destination: (.+)""").findAll(output).lastOrNull()?.groupValues?.get(1)
  }

  private fun extractOutputPathFromLine(line: String): String? {
    val trimmed = line.trim()
    if (trimmed.isEmpty()) return null

    fun after(prefix: String): String? {
      if (!trimmed.contains(prefix)) return null
      val p = trimmed.substringAfter(prefix).trim().trim('"')
      return if (p.isBlank()) null else p
    }

    return after("Destination:")
      ?: Regex("""\[Merger\] Merging formats into \"(.+?)\"""").find(trimmed)?.groupValues?.get(1)
      ?: Regex("""\[ffmpeg\] Destination: (.+)""").find(trimmed)?.groupValues?.get(1)
      ?: Regex("""\[ExtractAudio\] Destination: (.+)""").find(trimmed)?.groupValues?.get(1)
  }

  private fun scanLatestOutputFile(dir: File, sinceMs: Long): String? {
    return try {
      if (!dir.exists() || !dir.isDirectory) return null

      val files = dir.listFiles() ?: return null
      val latest = files
        .asSequence()
        .filter { it.isFile }
        .filter { f ->
          val name = f.name.lowercase()
          // Skip common temp / non-artifact files.
          !(name.endsWith(".part") || name.endsWith(".tmp") || name.endsWith(".ytdl"))
        }
        .filter { it.lastModified() >= sinceMs - 5_000 } // allow small clock skew
        .maxByOrNull { it.lastModified() }

      latest?.absolutePath
    } catch (_: Exception) {
      null
    }
  }

  private fun tryStatSizeBytes(path: String): Long? {
    return try {
      if (path.startsWith("content://")) {
        val uri = Uri.parse(path)
        contentResolver.openFileDescriptor(uri, "r")?.use { pfd ->
          val s = pfd.statSize
          if (s >= 0) s else null
        }
      } else {
        val f = File(path)
        if (f.exists()) f.length() else null
      }
    } catch (_: Exception) {
      null
    }
  }

  private fun tryExt(path: String): String? {
    val s = path.trim()
    if (s.isBlank()) return null
    val idx = s.lastIndexOf('.')
    if (idx <= 0 || idx == s.length - 1) return null
    val ext = s.substring(idx + 1).lowercase().trim()
    return if (ext.isBlank()) null else ext
  }

  private fun emitStarted(jobId: String, stepId: String, title: String) {
    sendJobEvent(
      JSONObject().apply {
        put("type", "Started")
        put("job_id", jobId)
        put("step_id", stepId)
        put("title", title)
        put("command", "yt-dlp")
        put("args", JSONArray())
        put("at_ms", System.currentTimeMillis())
      }
    )
  }

  private fun emitProgress(jobId: String, stepId: String, phase: String, percent: Float?, etaSeconds: Long?) {
    sendJobEvent(
      JSONObject().apply {
        put("type", "Progress")
        put("job_id", jobId)
        put("step_id", stepId)
        put("phase", phase)
        if (percent != null && percent.isFinite()) put("fraction", percent / 100.0) else put("fraction", JSONObject.NULL)
        if (etaSeconds != null && etaSeconds >= 0) put("eta_ms", etaSeconds * 1000L) else put("eta_ms", JSONObject.NULL)
        put("speed_bps", JSONObject.NULL)
        put("downloaded_bytes", JSONObject.NULL)
        put("total_bytes", JSONObject.NULL)
        put("at_ms", System.currentTimeMillis())
      }
    )
  }

  private fun emitLog(jobId: String, stepId: String, level: String, message: String) {
    sendJobEvent(
      JSONObject().apply {
        put("type", "Log")
        put("job_id", jobId)
        put("step_id", stepId)
        put("level", level)
        put("message", message)
        put("at_ms", System.currentTimeMillis())
      }
    )
  }

  private fun emitArtifact(jobId: String, stepId: String, path: String) {
    val sizeBytes = tryStatSizeBytes(path)
    val ext = tryExt(path)
    sendJobEvent(
      JSONObject().apply {
        put("type", "Artifact")
        put("job_id", jobId)
        put("step_id", stepId)
        put("path", path)
        put("kind", "file")
        if (sizeBytes != null && sizeBytes > 0) put("size_bytes", sizeBytes) else put("size_bytes", JSONObject.NULL)
        if (!ext.isNullOrBlank()) put("ext", ext) else put("ext", JSONObject.NULL)
        put("at_ms", System.currentTimeMillis())
      }
    )
  }

  private fun emitFinished(jobId: String, stepId: String, ok: Boolean, message: String?) {
    sendJobEvent(
      JSONObject().apply {
        if (ok) {
          put("type", "Finished")
          put("job_id", jobId)
          put("step_id", stepId)
          put("exit_code", 0)
          put("at_ms", System.currentTimeMillis())
        } else {
          put("type", "Failed")
          put("job_id", jobId)
          put("step_id", stepId)
          put("error", message ?: "Download failed")
          put("at_ms", System.currentTimeMillis())
        }
      }
    )
  }

  private fun emitCancelled(jobId: String, stepId: String) {
    sendJobEvent(
      JSONObject().apply {
        put("type", "Cancelled")
        put("job_id", jobId)
        put("step_id", stepId)
        put("reason", "cancelled")
        put("at_ms", System.currentTimeMillis())
      }
    )
  }

  inner class AndroidYtDlpBridge {
    @JavascriptInterface
    fun isReady(): Boolean = ytdlInitialized

    @JavascriptInterface
    fun getVersion(): String {
      return try {
        // Library exposes yt-dlp version via its bundled binary; if not available, return empty.
        YoutubeDL.getInstance().versionName(application) ?: ""
      } catch (_: Exception) {
        ""
      }
    }

    @JavascriptInterface
    fun getVideoInfo(url: String, callbackName: String) {
      getVideoInfoInternal(url, null, callbackName)
    }

    @JavascriptInterface
    fun getVideoInfoWithClient(url: String, youtubePlayerClient: String?, callbackName: String) {
      getVideoInfoInternal(url, youtubePlayerClient, callbackName)
    }

    private fun getVideoInfoInternal(url: String, youtubePlayerClient: String?, callbackName: String) {
      infoExecutor.execute {
        if (!ytdlInitialized) {
          sendCallback(callbackName, JSONObject().apply { put("error", "not_initialized") }.toString())
          return@execute
        }
        try {
          val request = YoutubeDLRequest(url)
          request.addOption("-J")
          request.addOption("--no-playlist")
          request.addOption("--encoding", "utf-8")
          if (!youtubePlayerClient.isNullOrBlank()) {
            request.addOption(
              "--extractor-args",
              "youtube:player_client=${youtubePlayerClient};player_skip=webpage,configs"
            )
          }
          val response = YoutubeDL.getInstance().execute(request)
          if (response.exitCode == 0) {
            sendCallback(callbackName, response.out ?: "{}")
          } else {
            sendCallback(
              callbackName,
              JSONObject().apply { put("error", response.err ?: "yt-dlp failed") }.toString()
            )
          }
        } catch (e: Exception) {
          sendCallback(callbackName, JSONObject().apply { put("error", e.message ?: "unknown") }.toString())
        }
      }
    }

    @JavascriptInterface
    fun getPlaylistInfo(url: String, callbackName: String) {
      getPlaylistInfoInternal(url, null, callbackName)
    }

    @JavascriptInterface
    fun getPlaylistInfoWithClient(url: String, youtubePlayerClient: String?, callbackName: String) {
      getPlaylistInfoInternal(url, youtubePlayerClient, callbackName)
    }

    private fun getPlaylistInfoInternal(url: String, youtubePlayerClient: String?, callbackName: String) {
      infoExecutor.execute {
        if (!ytdlInitialized) {
          sendCallback(callbackName, JSONObject().apply { put("error", "not_initialized") }.toString())
          return@execute
        }
        try {
          val request = YoutubeDLRequest(url)
          request.addOption("-J")
          request.addOption("--flat-playlist")
          request.addOption("--encoding", "utf-8")
          if (!youtubePlayerClient.isNullOrBlank()) {
            request.addOption(
              "--extractor-args",
              "youtube:player_client=${youtubePlayerClient};player_skip=webpage,configs"
            )
          }

          val response = YoutubeDL.getInstance().execute(request)
          if (response.exitCode != 0) {
            sendCallback(
              callbackName,
              JSONObject().apply { put("error", response.err ?: "yt-dlp failed") }.toString()
            )
            return@execute
          }

          val raw = response.out ?: "{}"
          val root = JSONObject(raw)
          val entriesArr = root.optJSONArray("entries") ?: JSONArray()

          val mappedEntries = JSONArray()
          for (i in 0 until entriesArr.length()) {
            val entry = entriesArr.optJSONObject(i) ?: continue
            val id = entry.optString("id", "")
            val title = entry.optString("title", "")
            val webpageUrl = entry.optString("url", entry.optString("webpage_url", ""))

            mappedEntries.put(
              JSONObject().apply {
                put("id", id)
                put("url", webpageUrl)
                put("title", title)
                put("duration", if (entry.has("duration")) entry.optDouble("duration") else JSONObject.NULL)
                put("thumbnail", entry.optString("thumbnail", JSONObject.NULL.toString()).let { if (it == "null") JSONObject.NULL else it })
                put("uploader", entry.optString("uploader", JSONObject.NULL.toString()).let { if (it == "null") JSONObject.NULL else it })
                put("is_music", false)
              }
            )
          }

          val playlistJson = JSONObject().apply {
            put("is_playlist", root.optBoolean("_type", false) || root.has("entries"))
            put("id", root.optString("id", JSONObject.NULL.toString()).let { if (it == "null" || it.isBlank()) JSONObject.NULL else it })
            put("title", root.optString("title", "Unknown"))
            put("uploader", root.optString("uploader", JSONObject.NULL.toString()).let { if (it == "null" || it.isBlank()) JSONObject.NULL else it })
            put("thumbnail", root.optString("thumbnail", JSONObject.NULL.toString()).let { if (it == "null" || it.isBlank()) JSONObject.NULL else it })
            put("total_count", mappedEntries.length())
            put("entries", mappedEntries)
            put("has_more", false)
          }

          sendCallback(callbackName, playlistJson.toString())
        } catch (e: Exception) {
          sendCallback(callbackName, JSONObject().apply { put("error", e.message ?: "unknown") }.toString())
        }
      }
    }

    @JavascriptInterface
    fun startDownloadJob(jobId: String, url: String, settingsJson: String) {
      downloadExecutor.execute {
        val stepId = "download"
        var title = "Downloading..."
        emitStarted(jobId, stepId, title)

        if (!ytdlInitialized) {
          emitFinished(jobId, stepId, false, "not_initialized")
          return@execute
        }

        try {
          val settings = JSONObject(settingsJson)
          val format = settings.optString("format", "best")
          val playlistFolder = settings.optString("playlistFolder", "").ifBlank { null }
          val isAudioOnly = settings.optBoolean("isAudioOnly", false)

          val aria2Connections = settings.optInt("aria2Connections", 8)
          val aria2Splits = settings.optInt("aria2Splits", 8)
          val aria2MinSplitSize = settings.optString("aria2MinSplitSize", "1M")
          val speedLimit = settings.optInt("speedLimit", 0)
          val downloadPath = settings.optString("downloadPath", "").ifBlank { null }
          val youtubePlayerClient = settings.optString("youtubePlayerClient", "").ifBlank { null }
          val outputTemplate = settings.optString("outputTemplate", "").ifBlank { null }

          val embedThumbnail = settings.optBoolean("embedThumbnail", true)
          val embedChapters = settings.optBoolean("embedChapters", true)
          val embedSubtitles = settings.optBoolean("embedSubtitles", false)
          val subtitleLanguages = settings.optString("subtitleLanguages", "").ifBlank { null }
          val sponsorBlock = settings.optBoolean("sponsorBlock", false)
          val sponsorBlockCategories = settings.optJSONArray("sponsorBlockCategories") ?: JSONArray()
          val remux = settings.optBoolean("remux", true)
          val convertToMp4 = settings.optBoolean("convertToMp4", false)
          val clipRanges = settings.optJSONArray("clipRanges")

          val baseDir = if (!downloadPath.isNullOrBlank()) {
            File(downloadPath)
          } else {
            File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Comine")
          }

          val dlDir = if (!playlistFolder.isNullOrBlank()) File(baseDir, safePlaylistFolderName(playlistFolder)) else baseDir
          if (!dlDir.exists()) dlDir.mkdirs()

          val scanStartMs = System.currentTimeMillis()
          var artifactPathCandidate: String? = null

          val baseTemplate = outputTemplate ?: "%(title)s.%(ext)s"
          val request = YoutubeDLRequest(url)
          request.addOption("-o", dlDir.absolutePath + "/" + baseTemplate)
          request.addOption("--encoding", "utf-8")

          if (!format.isNullOrBlank() && format != "best") request.addOption("-f", format)

          if (!youtubePlayerClient.isNullOrBlank()) {
            request.addOption(
              "--extractor-args",
              "youtube:player_client=${youtubePlayerClient};player_skip=webpage,configs"
            )
          }

          if (clipRanges != null && clipRanges.length() > 0) {
            for (i in 0 until clipRanges.length()) {
              val r = clipRanges.optJSONObject(i) ?: continue
              val start = r.optDouble("start", 0.0)
              val end = r.optDouble("end", -1.0)
              if (end > start) {
                request.addOption("--download-sections", "*${start}-${end}")
              }
            }

            // Helps avoid brief A/V desync at cut boundaries by ensuring keyframes at cuts.
            if (ffmpegAvailable) {
              request.addOption("--force-keyframes-at-cuts")
            }
          }

          if (isAudioOnly) {
            request.addOption("-x")
            request.addOption("--audio-format", "m4a")
            if (ffmpegAvailable && embedThumbnail) {
              request.addOption("--embed-thumbnail")
              request.addOption("--convert-thumbnails", "jpg")
            }
          } else if (ffmpegAvailable && remux) {
            if (convertToMp4) {
              request.addOption("--recode-video", "mp4")
            } else {
              request.addOption("--remux-video", "mp4")
            }
          }

          if (ffmpegAvailable && embedChapters) {
            request.addOption("--embed-chapters")
          }

          if (ffmpegAvailable && embedSubtitles) {
            request.addOption("--write-subs")
            request.addOption("--write-auto-subs")
            if (!subtitleLanguages.isNullOrBlank()) request.addOption("--sub-langs", subtitleLanguages)
            request.addOption("--embed-subs")
          }

          if (sponsorBlock) {
            val cats = mutableListOf<String>()
            for (i in 0 until sponsorBlockCategories.length()) {
              val c = sponsorBlockCategories.optString(i)
              if (!c.isNullOrBlank()) cats.add(c)
            }
            if (cats.isNotEmpty()) {
              request.addOption("--sponsorblock-remove", cats.joinToString(","))
            } else {
              request.addOption("--sponsorblock-remove", "sponsor")
            }
          }

          if (aria2Available) {
            val connections = aria2Connections.coerceIn(1, 16)
            val splits = aria2Splits.coerceIn(1, 16)
            val minSplit = if (aria2MinSplitSize.isNullOrBlank()) "1M" else aria2MinSplitSize
            request.addOption("--downloader", "libaria2c.so")
            request.addOption("--external-downloader-args", "aria2c:'-x $connections -s $splits -k $minSplit'")
          }

          if (speedLimit > 0) request.addOption("--limit-rate", "${speedLimit}M")

          val response = try {
            YoutubeDL.getInstance().execute(request, jobId) { p, eta, line ->
              title = title
              emitProgress(jobId, stepId, "download", p, eta?.toLong())
              if (!line.isNullOrBlank()) {
                emitLog(jobId, stepId, "debug", line)
                val maybe = extractOutputPathFromLine(line)
                if (!maybe.isNullOrBlank()) artifactPathCandidate = maybe
              }
            }
          } catch (_: Throwable) {
            YoutubeDL.getInstance().execute(request) { p, eta, line ->
              emitProgress(jobId, stepId, "download", p, eta?.toLong())
              if (!line.isNullOrBlank()) {
                emitLog(jobId, stepId, "debug", line)
                val maybe = extractOutputPathFromLine(line)
                if (!maybe.isNullOrBlank()) artifactPathCandidate = maybe
              }
            }
          }

          val out = response.out ?: ""
          val err = response.err ?: ""
          val maybePath = artifactPathCandidate
            ?: bestGuessOutputPath(out + "\n" + err)
            ?: scanLatestOutputFile(dlDir, scanStartMs)
          if (!maybePath.isNullOrBlank()) {
            emitArtifact(jobId, stepId, maybePath)
          }

          if (response.exitCode == 0) {
            emitFinished(jobId, stepId, true, null)
          } else {
            emitFinished(jobId, stepId, false, response.err ?: "exitCode=${response.exitCode}")
          }
        } catch (e: Exception) {
          emitFinished(jobId, stepId, false, e.message ?: "unknown")
        }
      }
    }

    @JavascriptInterface
    fun startDownloadJobWithOptions(jobId: String, optionsJson: String, outputConfigJson: String) {
      downloadExecutor.execute {
        val stepId = "download"
        emitStarted(jobId, stepId, "Downloading...")

        if (!ytdlInitialized) {
          emitFinished(jobId, stepId, false, "not_initialized")
          return@execute
        }

        try {
          val outputCfg = JSONObject(outputConfigJson)
          val downloadPath = outputCfg.optString("downloadPath", "").ifBlank { null }
          val playlistFolder = outputCfg.optString("playlistFolder", "").ifBlank { null }

          val baseDir = if (!downloadPath.isNullOrBlank()) {
            File(downloadPath)
          } else {
            File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Comine")
          }

          val dlDir = if (!playlistFolder.isNullOrBlank()) File(baseDir, safePlaylistFolderName(playlistFolder)) else baseDir
          if (!dlDir.exists()) dlDir.mkdirs()

          val scanStartMs = System.currentTimeMillis()
          var artifactPathCandidate: String? = null

          // Apply Rust-built option list.
          val optsArr = JSONArray(optionsJson)

          // Rebuild request now that we know the URL.
          var url: String? = null
          for (i in 0 until optsArr.length()) {
            val obj = optsArr.optJSONObject(i) ?: continue
            if (obj.optString("key") == "__URL__") {
              url = obj.optString("value", "").ifBlank { null }
              break
            }
          }
          if (url.isNullOrBlank()) {
            emitFinished(jobId, stepId, false, "missing_url")
            return@execute
          }

          val realReq = YoutubeDLRequest(url)
          for (i in 0 until optsArr.length()) {
            val obj = optsArr.optJSONObject(i) ?: continue
            val key = obj.optString("key", "")
            if (key.isBlank() || key == "__URL__") continue
            val hasValue = obj.has("value") && !obj.isNull("value")
            var value = if (hasValue) obj.optString("value", "") else null

            // Replace output dir token.
            if (value != null && value.contains("__COMINE_OUTPUT_DIR__")) {
              value = value.replace("__COMINE_OUTPUT_DIR__", dlDir.absolutePath)
            }

            // If embedded aria2 isn't available, drop downloader options.
            if (!aria2Available && (key == "--downloader" || key == "--external-downloader-args" || key == "--downloader-args")) {
              continue
            }
            // If ffmpeg isn't available, drop options that would fail.
            if (!ffmpegAvailable && (key == "--remux-video" || key == "--recode-video" || key == "--embed-thumbnail" || key == "--embed-subs" || key == "--embed-chapters")) {
              continue
            }

            if (value == null) {
              realReq.addOption(key)
            } else {
              realReq.addOption(key, value)
            }
          }

          val response = try {
            YoutubeDL.getInstance().execute(realReq, jobId) { p, eta, line ->
              emitProgress(jobId, stepId, "download", p, eta?.toLong())
              if (!line.isNullOrBlank()) {
                emitLog(jobId, stepId, "debug", line)
                if (line.contains(">>>FILEPATH:")) {
                  val idx = line.indexOf(">>>FILEPATH:")
                  val path = line.substring(idx + ">>>FILEPATH:".length).trim()
                  if (path.isNotBlank()) artifactPathCandidate = path
                } else {
                  val maybe = extractOutputPathFromLine(line)
                  if (!maybe.isNullOrBlank()) artifactPathCandidate = maybe
                }
              }
            }
          } catch (_: Throwable) {
            YoutubeDL.getInstance().execute(realReq) { p, eta, line ->
              emitProgress(jobId, stepId, "download", p, eta?.toLong())
              if (!line.isNullOrBlank()) {
                emitLog(jobId, stepId, "debug", line)
                if (line.contains(">>>FILEPATH:")) {
                  val idx = line.indexOf(">>>FILEPATH:")
                  val path = line.substring(idx + ">>>FILEPATH:".length).trim()
                  if (path.isNotBlank()) artifactPathCandidate = path
                } else {
                  val maybe = extractOutputPathFromLine(line)
                  if (!maybe.isNullOrBlank()) artifactPathCandidate = maybe
                }
              }
            }
          }

          val out = response.out ?: ""
          val err = response.err ?: ""
          val maybePath = artifactPathCandidate
            ?: bestGuessOutputPath(out + "\n" + err)
            ?: scanLatestOutputFile(dlDir, scanStartMs)
          if (!maybePath.isNullOrBlank()) {
            emitArtifact(jobId, stepId, maybePath)
          }

          if (response.exitCode == 0) {
            emitFinished(jobId, stepId, true, null)
          } else {
            emitFinished(jobId, stepId, false, response.err ?: "exitCode=${response.exitCode}")
          }
        } catch (e: Exception) {
          emitFinished(jobId, stepId, false, e.message ?: "unknown")
        }
      }
    }

    @JavascriptInterface
    fun cancelJob(jobId: String): Boolean {
      val stepId = "download"
      return try {
        val killedYtdlp = YoutubeDL.getInstance().destroyProcessById(jobId)
        val ffmpeg = jobIdToFfmpegProcess.remove(jobId)
        if (ffmpeg != null) {
          try {
            ffmpeg.destroy()
          } catch (_: Exception) {
          }
        }
        if (killedYtdlp || ffmpeg != null) {
          emitCancelled(jobId, stepId)
        }
        killedYtdlp || ffmpeg != null
      } catch (e: Exception) {
        sendLog("warn", "cancelJob($jobId) failed: ${e.message}")
        false
      }
    }

    @JavascriptInterface
    fun openFile(filePath: String): Boolean {
      return try {
        val uri = if (filePath.startsWith("content://")) {
          Uri.parse(filePath)
        } else {
          val file = File(filePath)
          if (!file.exists()) return false
          FileProvider.getUriForFile(this@MainActivity, BuildConfig.APPLICATION_ID + ".fileprovider", file)
        }

        val mime = contentResolver.getType(uri)
          ?: URLConnection.guessContentTypeFromName(filePath)
          ?: "*/*"

        val intent = Intent(Intent.ACTION_VIEW).apply {
          setDataAndType(uri, mime)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
          addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(intent)
        true
      } catch (e: Exception) {
        sendLog("warn", "openFile failed: ${e.message}")
        false
      }
    }

    @JavascriptInterface
    fun openFolder(filePath: String): Boolean {
      return try {
        val file = File(filePath)
        val folder = if (file.isDirectory) file else file.parentFile
        if (folder == null || !folder.exists()) return false

        val uri = FileProvider.getUriForFile(this@MainActivity, BuildConfig.APPLICATION_ID + ".fileprovider", folder)
        val intent = Intent(Intent.ACTION_VIEW).apply {
          setDataAndType(uri, "resource/folder")
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
          addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(intent)
        true
      } catch (e: Exception) {
        sendLog("warn", "openFolder failed: ${e.message}")
        false
      }
    }

    @JavascriptInterface
    fun pickFile(mimeTypes: String, callbackName: String) {
      filePickerCallback = callbackName
      val types = mimeTypes.split(',').map { it.trim() }.filter { it.isNotEmpty() }.toTypedArray()
      mainHandler.post { filePickerLauncher.launch(if (types.isNotEmpty()) types else arrayOf("*/*")) }
    }

    @JavascriptInterface
    fun pickFolder(callbackName: String) {
      folderPickerCallback = callbackName
      mainHandler.post { folderPickerLauncher.launch(null) }
    }

    @JavascriptInterface
    fun processYtmThumbnail(thumbnailUrl: String, callbackName: String) {
      infoExecutor.execute {
        try {
          val url = URL(thumbnailUrl)
          val conn = (url.openConnection() as HttpURLConnection).apply {
            connectTimeout = 15000
            readTimeout = 15000
            instanceFollowRedirects = true
          }
          conn.connect()
          val bytes = conn.inputStream.use { it.readBytes() }
          val bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
          if (bmp == null) {
            sendCallback(callbackName, JSONObject().apply { put("url", thumbnailUrl) }.toString())
            return@execute
          }

          val size = minOf(bmp.width, bmp.height)
          val x = (bmp.width - size) / 2
          val y = (bmp.height - size) / 2
          val cropped = if (bmp.width == bmp.height) bmp else Bitmap.createBitmap(bmp, x, y, size, size)

          if (cropped.width == bmp.width && cropped.height == bmp.height) {
            sendCallback(callbackName, JSONObject().apply { put("url", thumbnailUrl) }.toString())
            return@execute
          }

          val out = ByteArrayOutputStream()
          cropped.compress(Bitmap.CompressFormat.JPEG, 92, out)
          val b64 = Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
          val dataUri = "data:image/jpeg;base64,$b64"
          sendCallback(callbackName, JSONObject().apply { put("url", dataUri) }.toString())
        } catch (_: Exception) {
          sendCallback(callbackName, JSONObject().apply { put("url", thumbnailUrl) }.toString())
        }
      }
    }
  }

  inner class AndroidColorsBridge(private val context: Context) {
    @JavascriptInterface
    fun getMaterialColors(): String {
      return try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
          val primary = android.R.color.system_accent1_500
          val secondary = android.R.color.system_accent2_500
          val tertiary = android.R.color.system_accent3_500

          val primaryColor = context.getColor(primary)
          val secondaryColor = context.getColor(secondary)
          val tertiaryColor = context.getColor(tertiary)

          val result = JSONObject().apply {
            put("primary", String.format("#%06X", 0xFFFFFF and primaryColor))
            put("secondary", String.format("#%06X", 0xFFFFFF and secondaryColor))
            put("tertiary", String.format("#%06X", 0xFFFFFF and tertiaryColor))
          }

          Log.d(TAG, "Material You colors: $result")
          result.toString()
        } else {
          JSONObject().apply {
            put("primary", "#6366F1")
          }.toString()
        }
      } catch (e: Exception) {
        Log.e(TAG, "Failed to get Material colors", e)
        "{}"
      }
    }

    @JavascriptInterface
    fun getWallpaperColors(): String {
      return try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
          val wallpaperManager = android.app.WallpaperManager.getInstance(context)
          val colors = wallpaperManager.getWallpaperColors(android.app.WallpaperManager.FLAG_SYSTEM)

          if (colors != null) {
            val result = JSONObject()
            colors.primaryColor?.let {
              result.put("primary", String.format("#%06X", 0xFFFFFF and it.toArgb()))
            }
            colors.secondaryColor?.let {
              result.put("secondary", String.format("#%06X", 0xFFFFFF and it.toArgb()))
            }
            colors.tertiaryColor?.let {
              result.put("tertiary", String.format("#%06X", 0xFFFFFF and it.toArgb()))
            }
            Log.d(TAG, "Wallpaper colors: $result")
            return result.toString()
          }
        }
        "{}"
      } catch (e: Exception) {
        Log.e(TAG, "Failed to get wallpaper colors", e)
        "{}"
      }
    }
  }
}

