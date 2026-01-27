package com.nichind.comine

import android.content.ComponentName
import android.content.ServiceConnection
import android.os.Build
import android.os.IBinder
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
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

        @Volatile private var instance: MainActivity? = null

        fun currentInstance(): MainActivity? = instance
    }

    private var webView: WebView? = null
    private val mainHandler = Handler(Looper.getMainLooper())
    private val downloadExecutor = Executors.newFixedThreadPool(3)
    private val infoExecutor = Executors.newCachedThreadPool()

    private var pendingShareUrl: String? = null
    private var folderPickerCallback: String? = null
    private var filePickerCallback: String? = null

    private lateinit var folderPickerLauncher: ActivityResultLauncher<Uri?>
    private lateinit var filePickerLauncher: ActivityResultLauncher<Array<String>>

    private val pendingCallbackData = ConcurrentHashMap<String, String>()

    private var downloadService: DownloadService? = null
    private var isBound = false

    private val connection = object : ServiceConnection {
        override fun onServiceConnected(className: ComponentName, service: IBinder) {
            val binder = service as DownloadService.LocalBinder
            downloadService = binder.getService()
            isBound = true
        }
        override fun onServiceDisconnected(arg0: ComponentName) {
            isBound = false
        }
    }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()

    instance = this

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

        RustBridge.initialize()

        Intent(this, DownloadService::class.java).also { intent ->
            bindService(intent, connection, Context.BIND_AUTO_CREATE)
        }

            YtDlp.init(application) { dispatchYtdlpReady() }
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

        if (!RustBridge.isReady()) {
            RustBridge.initialize()
        }

        if (YtDlp.initialized) {
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
        if (instance === this) instance = null
        if (isBound) {
            unbindService(connection)
            isBound = false
        }
        try { downloadExecutor.shutdownNow() } catch (_: Exception) {}
        try { infoExecutor.shutdownNow() } catch (_: Exception) {}
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
    evalJs("(function(){try{window.dispatchEvent(new CustomEvent('share-intent',{detail:{url:$urlLit}}));}catch(e){}})();")
  }

  private fun dispatchYtdlpReady() {
    evalJs("(function(){try{window.__YTDLP_READY__=true;window.dispatchEvent(new Event('ytdlp-ready'));}catch(e){}})();")
  }

  private fun evalJs(js: String) {
    mainHandler.post {
      try { webView?.evaluateJavascript(js, null) } catch (_: Exception) {}
    }
  }

  private fun sendCallback(callbackName: String, arg: String) {
    if (arg.length > 50_000) {
      sendCallbackLarge(callbackName, arg)
      return
    }
    val cb = JSONObject.quote(callbackName)
    val argLit = JSONObject.quote(arg)
    evalJs("(function(){try{var fn=window[$cb];if(typeof fn==='function')fn($argLit);}catch(e){console.error('sendCallback error:',e);}})();")
  }

  private fun sendCallbackLarge(callbackName: String, data: String) {
    val dataKey = "cb_data_${System.currentTimeMillis()}_${(Math.random() * 100000).toInt()}"
    pendingCallbackData[dataKey] = data
    val cb = JSONObject.quote(callbackName)
    val keyLit = JSONObject.quote(dataKey)
    evalJs("(function(){try{var fn=window[$cb];if(typeof fn==='function'){var data=window.AndroidYtDlp.fetchCallbackData($keyLit);fn(data);}}catch(e){console.error('sendCallbackLarge error:',e);}})();")
  }

  private fun sendCallbackNull(callbackName: String) {
    val cb = JSONObject.quote(callbackName)
    evalJs("(function(){try{var fn=window[$cb];if(typeof fn==='function')fn(null);}catch(e){}})();")
  }

  fun startDownloadFromRust(jobId: String, requestJson: String) {
    Log.d(TAG, "startDownloadFromRust: jobId=$jobId")
    try {
      val intent = Intent(this, DownloadService::class.java).apply {
        action = DownloadService.ACTION_START
        putExtra(DownloadService.EXTRA_JOB_ID, jobId)
        putExtra(DownloadService.EXTRA_REQUEST_JSON, requestJson)
      }
      startForegroundService(intent)
    } catch (e: Exception) {
      Log.e(TAG, "startDownloadFromRust failed", e)
      RustBridge.notifyFailed(jobId, "Failed to start download service: ${e.message}")
    }
  }

  fun pauseDownloadFromRust(jobId: String) {
    Log.d(TAG, "pauseDownloadFromRust: jobId=$jobId")
    try {
      val intent = Intent(this, DownloadService::class.java).apply {
        action = DownloadService.ACTION_PAUSE
        putExtra(DownloadService.EXTRA_JOB_ID, jobId)
      }
      startService(intent)
    } catch (e: Exception) {
      Log.e(TAG, "pauseDownloadFromRust failed", e)
    }
  }

  fun cancelDownloadFromRust(jobId: String) {
    Log.d(TAG, "cancelDownloadFromRust: jobId=$jobId")
    try {
      val intent = Intent(this, DownloadService::class.java).apply {
        action = DownloadService.ACTION_CANCEL
        putExtra(DownloadService.EXTRA_JOB_ID, jobId)
      }
      startService(intent)
    } catch (e: Exception) {
      Log.e(TAG, "cancelDownloadFromRust failed", e)
    }
  }

  fun startAria2DownloadFromRust(jobId: String, optsJson: String) {
    Log.d(TAG, "startAria2DownloadFromRust: jobId=$jobId")

    if (!YtDlp.aria2Available) {
      Log.e(TAG, "aria2 not available")
      RustBridge.notifyFailed(jobId, "aria2 not initialized")
      return
    }

    downloadExecutor.submit {
      try {
        val opts = JSONObject(optsJson)
        val result = Aria2.execute(
          context = this,
          jobId = jobId,
          url = opts.getString("url"),
          outputDir = opts.getString("output_dir"),
          outputFile = opts.optString("output_file", null),
          connections = opts.optInt("connections", 8),
          splits = opts.optInt("splits", 8),
          minSplitSize = opts.optString("min_split_size", "1M"),
          speedLimit = opts.optLong("speed_limit", 0),
          proxy = opts.optString("proxy", null),
          isTorrent = opts.optBoolean("is_torrent", false)
        )

        when (result) {
          is Aria2.ExecuteResult.Success -> {
            RustBridge.notifyCompleted(jobId, result.outputPath, result.title)
          }
          is Aria2.ExecuteResult.Failed -> {
            RustBridge.notifyFailed(jobId, result.error)
          }
          is Aria2.ExecuteResult.Cancelled -> {
            RustBridge.notifyCancelled(jobId)
          }
        }
      } catch (e: Exception) {
        Log.e(TAG, "aria2 download failed", e)
        RustBridge.notifyFailed(jobId, e.message ?: "Unknown error")
      }
    }
  }

  fun cancelAria2DownloadFromRust(jobId: String) {
    Log.d(TAG, "cancelAria2DownloadFromRust: jobId=$jobId")
    val cancelled = Aria2.cancel(jobId)
    if (cancelled) {
      RustBridge.notifyCancelled(jobId)
    }
  }

  fun convertFileWithFFmpeg(jobId: String, requestJson: String) {
    Log.d(TAG, "convertFileWithFFmpeg: jobId=$jobId")
    
    if (!YtDlp.ffmpegAvailable) {
      Log.e(TAG, "FFmpeg not available")
      RustBridge.notifyConvertFailed(jobId, "FFmpeg not initialized")
      return
    }
    
    downloadExecutor.submit {
      try {
        val request = JSONObject(requestJson)
        val sourcePath = request.getString("source_path")
        val targetFormat = request.getString("target_format")
        // JSONObject.optString returns "null" for JSON null.
        val outputDirectory = request.optString("output_directory", "").takeIf { it.isNotBlank() && it != "null" }
        val outputFilename = request.optString("output_filename", "").takeIf { it.isNotBlank() && it != "null" }
        val audioOnly = request.optBoolean("audio_only", false)
        
        Log.d(TAG, "Convert request: source=$sourcePath, format=$targetFormat, outDir=$outputDirectory, outFile=$outputFilename")
        
        val sourceFile = File(sourcePath).absoluteFile
        if (!sourceFile.exists()) {
          RustBridge.notifyConvertFailed(jobId, "Source file not found: $sourcePath")
          return@submit
        }
        
        Log.d(TAG, "Source file exists: ${sourceFile.absolutePath}")
        Log.d(TAG, "Parent file: ${sourceFile.parentFile?.absolutePath ?: "NULL"}")
        
        val outputDir: File = when {
          outputDirectory != null -> File(outputDirectory)
          sourceFile.parentFile != null -> sourceFile.parentFile!!
          sourcePath.contains("/") -> File(sourcePath.substringBeforeLast("/"))
          else -> File("/storage/emulated/0/Download")
        }
        
        val baseName = outputFilename ?: sourceFile.name
          .substringBeforeLast(".")
          .trim('"', ' ')
        
        var outputFile = File(outputDir, "$baseName.$targetFormat")
        
        Log.d(TAG, "Output dir: ${outputDir.absolutePath}")
        Log.d(TAG, "Base name: $baseName")
        Log.d(TAG, "Output file: ${outputFile.absolutePath}")
        
        if (outputFile.absolutePath == sourceFile.absolutePath) {
          outputFile = File(outputDir, "${baseName}_converted.$targetFormat")
        }
        
        var counter = 1
        while (outputFile.exists()) {
          outputFile = File(outputDir, "${baseName}_$counter.$targetFormat")
          counter++
        }
        
        val finalOutputPath = outputFile.absolutePath
        
        val duration = FFmpegExecutor.getDuration(applicationContext, sourcePath)
        Log.d(TAG, "Source duration: $duration seconds")
        
        val args = mutableListOf<String>()
        args.add("-y")
        args.add("-i")
        args.add(sourcePath)
        
        if (audioOnly) {
          args.add("-vn")
          when (targetFormat.lowercase()) {
            "mp3" -> {
              args.add("-c:a")
              args.add("libmp3lame")
              args.add("-q:a")
              args.add("0")
            }
            "aac", "m4a" -> {
              args.add("-c:a")
              args.add("aac")
              args.add("-b:a")
              args.add("256k")
            }
            "opus" -> {
              args.add("-c:a")
              args.add("libopus")
              args.add("-b:a")
              args.add("192k")
            }
            "flac" -> {
              args.add("-c:a")
              args.add("flac")
            }
            "wav" -> {
              args.add("-c:a")
              args.add("pcm_s16le")
            }
            else -> {
            }
          }
        } else {
          when (targetFormat.lowercase()) {
            "mp4" -> {
              args.add("-c:v")
              args.add("copy")
              args.add("-c:a")
              args.add("aac")
              args.add("-movflags")
              args.add("+faststart")
            }
            "mkv" -> {
              args.add("-c:v")
              args.add("copy")
              args.add("-c:a")
              args.add("copy")
            }
            "webm" -> {
              args.add("-c:v")
              args.add("copy")
              args.add("-c:a")
              args.add("libopus")
            }
            else -> {
              args.add("-c")
              args.add("copy")
            }
          }
        }
        
        args.add(finalOutputPath)
        
        Log.d(TAG, "Running FFmpeg: ${args.joinToString(" ")}")
        
        val result = FFmpegExecutor.execute(
          applicationContext,
          jobId,
          args,
          duration
        ) { progress, speed ->
          RustBridge.notifyConvertProgress(jobId, progress, speed)
        }
        
        when (result) {
          is FFmpegExecutor.ExecuteResult.Success -> {
            if (outputFile.exists()) {
              val filesize = outputFile.length()
              RustBridge.notifyConvertCompleted(jobId, finalOutputPath, filesize, targetFormat, null)
            } else {
              RustBridge.notifyConvertFailed(jobId, "Output file not created")
            }
          }
          is FFmpegExecutor.ExecuteResult.Failed -> {
            Log.e(TAG, "FFmpeg conversion failed: ${result.error}")
            RustBridge.notifyConvertFailed(jobId, result.error)
          }
          is FFmpegExecutor.ExecuteResult.Cancelled -> {
            Log.i(TAG, "FFmpeg conversion cancelled: $jobId")
            if (outputFile.exists()) {
              outputFile.delete()
            }
            RustBridge.notifyConvertFailed(jobId, "Cancelled")
          }
        }
        
      } catch (e: Exception) {
        Log.e(TAG, "convertFileWithFFmpeg failed", e)
        RustBridge.notifyConvertFailed(jobId, e.message ?: "Unknown error")
      }
    }
  }
  fun openFile(filePath: String): Boolean {
    Log.d(TAG, "openFile called with path: $filePath")
    return try {
      val uri = if (filePath.startsWith("content://")) {
        Uri.parse(filePath)
      } else {
        val file = File(filePath)
        Log.d(TAG, "File exists: ${file.exists()}, path: ${file.absolutePath}")
        if (!file.exists()) {
          Log.w(TAG, "File does not exist: $filePath")
          return false
        }
        FileProvider.getUriForFile(this, BuildConfig.APPLICATION_ID + ".fileprovider", file)
      }

      val mime = contentResolver.getType(uri)
        ?: URLConnection.guessContentTypeFromName(filePath)
        ?: "*/*"

      Log.d(TAG, "Opening file with URI: $uri, mime: $mime")

      val intent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(uri, mime)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      startActivity(intent)
      Log.d(TAG, "openFile succeeded")
      true
    } catch (e: Exception) {
      Log.w(TAG, "openFile failed: ${e.message}", e)
      false
    }
  }

  fun openFolder(filePath: String): Boolean {
    return try {
      val file = File(filePath)
      val folder = if (file.isDirectory) file else file.parentFile
      if (folder == null || !folder.exists()) return false

      try {
        val uri = FileProvider.getUriForFile(this, BuildConfig.APPLICATION_ID + ".fileprovider", folder)
        val intent = Intent(Intent.ACTION_VIEW).apply {
          setDataAndType(uri, "resource/folder")
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
          addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(intent)
        return true
      } catch (_: Exception) {
        val documentsIntent = Intent(Intent.ACTION_VIEW).apply {
          val folderUri = Uri.parse("content://com.android.externalstorage.documents/document/primary:${folder.absolutePath.removePrefix("/storage/emulated/0/")}")
          data = folderUri
          addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(documentsIntent)
        return true
      }
    } catch (e: Exception) {
      Log.w(TAG, "openFolder failed: ${e.message}")
      false
    }
  }

  inner class AndroidYtDlpBridge {
    @JavascriptInterface
    fun isReady(): Boolean = YtDlp.initialized

    @JavascriptInterface
    fun getVersion(): String = YtDlp.getVersion(application)

    @JavascriptInterface
    fun fetchCallbackData(dataKey: String): String {
      return pendingCallbackData.remove(dataKey) ?: "{\"error\":\"data_not_found\"}"
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
        if (!YtDlp.initialized) {
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
        if (!YtDlp.initialized) {
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
    fun cancelJob(jobId: String): Boolean {
      return try {
        cancelDownloadFromRust(jobId)
        true
      } catch (e: Exception) {
        Log.w(TAG, "cancelJob($jobId) failed: ${e.message}")
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
        Log.w(TAG, "openFile failed: ${e.message}")
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
        Log.w(TAG, "openFolder failed: ${e.message}")
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

