package com.nichind.comine

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.File

interface JobBackend {
    fun name(): String
    fun execute(context: Context, jobId: String, args: JSONObject, updateProgress: (Int, String?, Long?) -> Unit): JobResult
    fun cancel(jobId: String)
    fun notifyRustSuccess(jobId: String, result: JobResult.Success)
    fun notifyRustFailure(jobId: String, error: String)
}

sealed class JobResult {
    data class Success(val outputPath: String, val title: String? = null, val thumbnail: String? = null) : JobResult()
    data class Failed(val error: String) : JobResult()
    object Cancelled : JobResult()
}

object BackendRegistry {
    private val backends = mutableMapOf<String, JobBackend>()

    init {
        register(YtDlpBackend())
        register(Aria2Backend())
        register(FFmpegBackend())
    }

    fun register(backend: JobBackend) {
        backends[backend.name()] = backend
    }

    fun get(name: String): JobBackend? {
        return backends[name]
    }
}

class YtDlpBackend : JobBackend {
    override fun name(): String = "ytdlp"

    override fun execute(
        context: Context,
        jobId: String,
        args: JSONObject,
        updateProgress: (Int, String?, Long?) -> Unit
    ): JobResult {
        val url = args.optString("url")
        val argsJson = args.optString("args")
        val outputDir = args.optString("output_directory", null)
        
        if (url.isNullOrBlank()) {
             return JobResult.Failed("Missing URL in payload")
        }

        val result = YtDlp.execute(
            jobId = jobId,
            url = url,
            argsJson = argsJson,
            outputDir = outputDir,
            onProgress = { progress, eta ->
                updateProgress(progress, null, eta)
            },
            onRawOutput = { line ->
                RustBridge.notifyRawOutput(jobId, line)
            }
        )
        
        return when (result) {
            is YtDlp.ExecuteResult.Success -> JobResult.Success(result.outputPath, result.title, result.thumbnailUrl)
            is YtDlp.ExecuteResult.Failed -> JobResult.Failed(result.error)
        }
    }

    override fun cancel(jobId: String) {
        YtDlp.cancel(jobId)
    }

    override fun notifyRustSuccess(jobId: String, result: JobResult.Success) {
        RustBridge.notifyCompleted(jobId, result.outputPath, result.title, result.thumbnail)
    }

    override fun notifyRustFailure(jobId: String, error: String) {
        RustBridge.notifyFailed(jobId, error)
    }
}

class Aria2Backend : JobBackend {
    override fun name(): String = "aria2"

    override fun execute(
        context: Context,
        jobId: String,
        args: JSONObject,
        updateProgress: (Int, String?, Long?) -> Unit
    ): JobResult {
        val argsJson = args.optString("args", "[]")
        
        val result = Aria2.execute(
            context = context,
            jobId = jobId,
            argsJson = argsJson,
            onRawOutput = { line -> RustBridge.notifyRawOutput(jobId, line) }
        )
        
        return when (result) {
            is Aria2.ExecuteResult.Success -> JobResult.Success(result.outputPath)
            is Aria2.ExecuteResult.Failed -> JobResult.Failed(result.error)
            is Aria2.ExecuteResult.Cancelled -> JobResult.Cancelled
        }
    }

    override fun cancel(jobId: String) {
        Aria2.cancel(jobId)
    }

    override fun notifyRustSuccess(jobId: String, result: JobResult.Success) {
        RustBridge.notifyCompleted(jobId, result.outputPath, result.title, result.thumbnail)
    }

    override fun notifyRustFailure(jobId: String, error: String) {
        RustBridge.notifyFailed(jobId, error)
    }
}

class FFmpegBackend : JobBackend {
    override fun name(): String = "ffmpeg"

    override fun execute(
        context: Context,
        jobId: String,
        args: JSONObject,
        updateProgress: (Int, String?, Long?) -> Unit
    ): JobResult {
        val sourcePath = args.optString("source_path")
        val targetFormat = args.optString("target_format")
        val outputDirectory = args.optString("output_directory", "").takeIf { it.isNotBlank() && it != "null" }
        val outputFilename = args.optString("output_filename", "").takeIf { it.isNotBlank() && it != "null" }
        val argsArr = args.optJSONArray("args")

        if (sourcePath.isNullOrBlank()) {
             return JobResult.Failed("Missing source_path")
        }

        val sourceFile = File(sourcePath).absoluteFile
        if (!sourceFile.exists()) {
            return JobResult.Failed("Source file not found: $sourcePath")
        }

        val outputDir: File = when {
            outputDirectory != null -> File(outputDirectory)
            sourceFile.parentFile != null -> sourceFile.parentFile!!
            sourcePath.contains("/") -> File(sourcePath.substringBeforeLast("/"))
            else -> File("/storage/emulated/0/Download/Comine")
        }

        val baseName = outputFilename ?: sourceFile.name
            .substringBeforeLast(".")
            .trim('"', ' ')

        val outputFile = FileUtils.getUniqueFile(outputDir, baseName, targetFormat)

        val finalOutputPath = outputFile.absolutePath
        val duration = FFmpegExecutor.getDuration(context, sourcePath)

        val cmd = mutableListOf<String>()
        cmd.add("-y")
        cmd.add("-i")
        cmd.add(sourcePath)

        if (argsArr != null) {
            for (i in 0 until argsArr.length()) {
                cmd.add(argsArr.getString(i))
            }
        }
        
        cmd.add(finalOutputPath)

        val result = FFmpegExecutor.execute(context, jobId, cmd, duration) { progress, speed ->
            RustBridge.notifyConvertProgress(jobId, progress, speed)
            updateProgress(progress.toInt(), speed, null)
        }

        return when (result) {
            is FFmpegExecutor.ExecuteResult.Success -> {
                if (outputFile.exists()) {
                   JobResult.Success(finalOutputPath) 
                } else {
                   JobResult.Failed("Output file not created")
                }
            }
            is FFmpegExecutor.ExecuteResult.Failed -> JobResult.Failed(result.error)
            is FFmpegExecutor.ExecuteResult.Cancelled -> JobResult.Cancelled
        }
    }

    override fun cancel(jobId: String) {
        FFmpegExecutor.cancel(jobId)
    }

    override fun notifyRustSuccess(jobId: String, result: JobResult.Success) {
        val file = File(result.outputPath)
        val filesize = if (file.exists()) file.length() else 0L
        val extension = file.extension
        
        RustBridge.notifyConvertCompleted(jobId, result.outputPath, filesize, extension, null)
    }

    override fun notifyRustFailure(jobId: String, error: String) {
        RustBridge.notifyConvertFailed(jobId, error)
    }
}
