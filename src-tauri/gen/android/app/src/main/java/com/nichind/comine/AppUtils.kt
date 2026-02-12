package com.nichind.comine

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.util.Base64
import android.util.Log
import androidx.core.content.FileProvider
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLConnection

object FileUtils {
    private const val TAG = "FileUtils"

    fun getUriForPath(context: Context, path: String): Uri? {
        return try {
            if (path.startsWith("content://")) {
                Uri.parse(path)
            } else {
                val file = File(path)
                FileProvider.getUriForFile(
                    context,
                    "${context.packageName}.fileprovider",
                    file
                )
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to get URI: ${e.message}")
            null
        }
    }

    fun openFile(context: Context, path: String): Boolean {
        try {
            val file = File(path)
            if (!path.startsWith("content://") && !file.exists()) {
                Log.w(TAG, "File does not exist: $path")
                return false
            }

            val uri = getUriForPath(context, path) ?: return false
            val mime = context.contentResolver.getType(uri)
                ?: URLConnection.guessContentTypeFromName(path)
                ?: "*/*"

            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mime)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            context.startActivity(intent)
            return true
        } catch (e: Exception) {
            Log.e(TAG, "openFile failed", e)
            return false
        }
    }

    fun openFolder(context: Context, path: String): Boolean {
        try {
            val file = File(path)
            val folder = if (file.isDirectory) file else file.parentFile
            if (folder == null || !folder.exists()) return false

            try {
                val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", folder)
                val intent = Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, "resource/folder")
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                context.startActivity(intent)
                return true
            } catch (e: Exception) {
                val documentsIntent = Intent(Intent.ACTION_VIEW).apply {
                     val storagePrefix = "/storage/emulated/0/"
                     val relPath = folder.absolutePath.removePrefix(storagePrefix)
                     val folderUri = Uri.parse("content://com.android.externalstorage.documents/document/primary:$relPath")
                     data = folderUri
                     addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                context.startActivity(documentsIntent)
                return true
            }
        } catch (e: Exception) {
            Log.e(TAG, "openFolder failed", e)
            return false
        }
    }

    fun getUniqueFile(dir: File, baseName: String, ext: String): File {
        var file = File(dir, "$baseName.$ext")
        var counter = 1
        while (file.exists()) {
            file = File(dir, "${baseName}_$counter.$ext")
            counter++
        }
        return file
    }
}

object ImageUtils {
    fun processYtmThumbnail(thumbnailUrl: String): String {
        try {
            val url = URL(thumbnailUrl)
            val conn = (url.openConnection() as HttpURLConnection).apply {
                connectTimeout = 15000
                readTimeout = 15000
                instanceFollowRedirects = true
            }
            conn.connect()
            val bytes = conn.inputStream.use { it.readBytes() }
            val bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return thumbnailUrl

            val size = minOf(bmp.width, bmp.height)
            val x = (bmp.width - size) / 2
            val y = (bmp.height - size) / 2
            val cropped = if (bmp.width == bmp.height) bmp else Bitmap.createBitmap(bmp, x, y, size, size)

            if (cropped.width == bmp.width && cropped.height == bmp.height) {
                // Was already square, or processing skipped
                return thumbnailUrl
            }

            val out = ByteArrayOutputStream()
            cropped.compress(Bitmap.CompressFormat.JPEG, 92, out)
            val b64 = Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
            return "data:image/jpeg;base64,$b64"
        } catch (e: Exception) {
            return thumbnailUrl
        }
    }
}

object ThemeUtils {
    private const val TAG = "ThemeUtils"

    fun getMaterialColors(context: Context): String {
      return try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
          val primary = android.R.color.system_accent1_500
          val secondary = android.R.color.system_accent2_500
          val tertiary = android.R.color.system_accent3_500

          val primaryColor = context.getColor(primary)
          val secondaryColor = context.getColor(secondary)
          val tertiaryColor = context.getColor(tertiary)

          JSONObject().apply {
            put("primary", String.format("#%06X", 0xFFFFFF and primaryColor))
            put("secondary", String.format("#%06X", 0xFFFFFF and secondaryColor))
            put("tertiary", String.format("#%06X", 0xFFFFFF and tertiaryColor))
          }.toString()
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

    fun getWallpaperColors(context: Context): String {
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
