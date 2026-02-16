package com.nichind.comine

import android.Manifest
import android.content.ComponentName
import android.content.Context
import android.content.ServiceConnection
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.storage.StorageManager
import android.provider.DocumentsContract
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Base64
import android.util.Log
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import com.yausername.youtubedl_android.YoutubeDL
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

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
    private var pendingNavigateTo: String? = null
    private var folderPickerCallback: String? = null
    private var folderPickerFuture: CompletableFuture<String?>? = null
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
        try {
             RustBridge.initialize()
             Log.i(TAG, "RustBridge initialized in onCreate")
        } catch (e: Exception) {
             Log.e(TAG, "RustBridge init onCreate failed", e)
        }
        
        DownloadNotifications.init(this)
        UpdateNotifications.init(this)
        maybeRequestPostNotifications()
    scheduleUpdateChecker()

    folderPickerLauncher = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
      val callbackName = folderPickerCallback
      folderPickerCallback = null
      val future = folderPickerFuture
      folderPickerFuture = null

      val resolvedPath: String? = if (uri != null) {
        try {
          contentResolver.takePersistableUriPermission(
            uri,
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
          )
        } catch (_: Exception) {
        }
        treeUriToFilePath(uri) ?: uri.toString()
      } else {
        null
      }

      future?.complete(resolvedPath)

      if (callbackName != null) {
        val resultJson = if (uri != null) {
          JSONObject().apply {
            put("success", true)
            put("uri", uri.toString())
            put("path", resolvedPath ?: uri.toString())
          }.toString()
        } else {
          JSONObject().apply {
            put("success", false)
            put("cancelled", true)
          }.toString()
        }
        sendCallback(callbackName, resultJson)
      }
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

        Intent(this, DownloadService::class.java).also { intent ->
            bindService(intent, connection, Context.BIND_AUTO_CREATE)
        }

            YtDlp.init(application) { dispatchYtdlpReady() }
        handleIntent(intent)
    }

        private fun maybeRequestPostNotifications() {
          if (Build.VERSION.SDK_INT < 33) return
          try {
            if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
              requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1001)
            }
          } catch (_: Exception) {
          }
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
        
        val navigateTo = pendingNavigateTo
        if (!navigateTo.isNullOrBlank()) {
            pendingNavigateTo = null
            mainHandler.postDelayed({ dispatchNavigateTo(navigateTo) }, 500)
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
    
    if (intent.action == UpdateNotifications.ACTION_OPEN_UPDATES) {
      val navigateTo = intent.getStringExtra(UpdateNotifications.EXTRA_NAVIGATE_TO)
      if (!navigateTo.isNullOrBlank()) {
        if (webView == null) pendingNavigateTo = navigateTo
        else dispatchNavigateTo(navigateTo)
      }
      UpdateNotifications.dismiss()
      return
    }
    
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

  private fun dispatchNavigateTo(path: String) {
    val pathLit = JSONObject.quote(path)
    evalJs("(function(){try{window.dispatchEvent(new CustomEvent('navigate-to',{detail:{path:$pathLit}}));}catch(e){}})();")
  }

  private fun scheduleUpdateChecker() {
    try {
      val updateCheckRequest = PeriodicWorkRequestBuilder<UpdateCheckWorker>(
        6, TimeUnit.HOURS, 15, TimeUnit.MINUTES
      ).build()

      WorkManager.getInstance(this).enqueueUniquePeriodicWork(
        UpdateCheckWorker.WORK_NAME,
        ExistingPeriodicWorkPolicy.KEEP,
        updateCheckRequest
      )
      Log.i(TAG, "Background update checker scheduled")
    } catch (e: Exception) {
      Log.e(TAG, "Failed to schedule update checker", e)
    }
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

  private fun treeUriToFilePath(uri: Uri): String? {
    return try {
      val docId = DocumentsContract.getTreeDocumentId(uri)
      val parts = docId.split(":", limit = 2)
      if (parts.size != 2) return null
      val volumeId = parts[0]
      val relativePath = parts[1]

      if (volumeId.equals("primary", ignoreCase = true)) {
        Environment.getExternalStorageDirectory().absolutePath + "/" + relativePath
      } else {
        "/storage/$volumeId/$relativePath"
      }
    } catch (e: Exception) {
      Log.w(TAG, "Failed to convert tree URI to file path: $uri", e)
      null
    }
  }

  fun pickFolderFromRust(): String? {
    val future = CompletableFuture<String?>()
    folderPickerFuture = future
    mainHandler.post { folderPickerLauncher.launch(null) }
    return try {
      future.get()
    } catch (e: Exception) {
      Log.w(TAG, "pickFolderFromRust failed", e)
      null
    }
  }

  fun startJobFromRust(jobId: String, backend: String, payloadJson: String, title: String) {
    Log.d(TAG, "startJobFromRust: jobId=$jobId, backend=$backend, title=$title")
    try {
      val intent = Intent(this, DownloadService::class.java).apply {
        action = DownloadService.ACTION_START
        putExtra(DownloadService.EXTRA_JOB_ID, jobId)
        putExtra(DownloadService.EXTRA_BACKEND, backend)
        putExtra(DownloadService.EXTRA_PAYLOAD, payloadJson)
        putExtra(DownloadService.EXTRA_TITLE, title)
      }
      startForegroundService(intent)
    } catch (e: Exception) {
      Log.e(TAG, "startJobFromRust failed", e)
      RustBridge.notifyFailed(jobId, "Failed to start service: ${e.message}")
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

  fun openFile(filePath: String): Boolean = FileUtils.openFile(this, filePath)
  fun openFolder(filePath: String): Boolean = FileUtils.openFolder(this, filePath)

  fun installApk(apkPath: String) {
    Log.i(TAG, "installApk: $apkPath")
    val file = java.io.File(apkPath)
    if (!file.exists()) {
      Log.e(TAG, "APK file does not exist: $apkPath")
      return
    }
    val uri = FileProvider.getUriForFile(this, "${packageName}.fileprovider", file)
    val intent = Intent(Intent.ACTION_VIEW).apply {
      setDataAndType(uri, "application/vnd.android.package-archive")
      addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    startActivity(intent)
  }

  inner class AndroidYtDlpBridge {
    @JavascriptInterface
    fun isReady(): Boolean = YtDlp.initialized

    @JavascriptInterface
    fun getVersion(): String = YtDlp.getVersion(application)

    @JavascriptInterface
    fun updateChannel(channel: String, callbackName: String) {
      infoExecutor.execute {
        try {
          val newVersion = YtDlp.updateChannel(application, channel)
          sendCallback(callbackName, JSONObject().apply {
            put("success", true)
            put("version", newVersion)
          }.toString())
        } catch (e: Exception) {
          sendCallback(callbackName, JSONObject().apply {
            put("error", e.message ?: "Update failed")
          }.toString())
        }
      }
    }

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
        val result = YtDlp.resolve(url, false, youtubePlayerClient)
        when (result) {
            is YtDlp.ResolveResult.Success -> sendCallback(callbackName, result.output)
            is YtDlp.ResolveResult.Failed -> sendCallback(callbackName, JSONObject().apply { put("error", result.error) }.toString())
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
        val result = YtDlp.resolve(url, true, youtubePlayerClient)
        if (result is YtDlp.ResolveResult.Failed) {
            sendCallback(callbackName, JSONObject().apply { put("error", result.error) }.toString())
            return@execute
        }

        val output = (result as YtDlp.ResolveResult.Success).output
        try {
          val raw = output.ifBlank { "{}" }
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
    fun openFile(filePath: String): Boolean = this@MainActivity.openFile(filePath)

    @JavascriptInterface
    fun openFolder(filePath: String): Boolean = this@MainActivity.openFolder(filePath)

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
    fun shareLogs(content: String) {
      try {
        val timestamp = java.text.SimpleDateFormat("yyyy-MM-dd_HH-mm-ss", java.util.Locale.US).format(java.util.Date())
        val file = File(cacheDir, "comine-logs-$timestamp.txt")
        file.writeText(content)
        val uri = FileProvider.getUriForFile(this@MainActivity, "${packageName}.fileprovider", file)
        val intent = Intent(Intent.ACTION_SEND).apply {
          type = "text/plain"
          putExtra(Intent.EXTRA_STREAM, uri)
          putExtra(Intent.EXTRA_SUBJECT, "Comine Logs $timestamp")
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        startActivity(Intent.createChooser(intent, "Share logs"))
      } catch (e: Exception) {
        Log.e(TAG, "Failed to share logs: ${e.message}")
      }
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
    fun getMaterialColors(): String = ThemeUtils.getMaterialColors(context)

    @JavascriptInterface
    fun getWallpaperColors(): String = ThemeUtils.getWallpaperColors(context)
  }
}