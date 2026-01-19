package dev.drop.receiver.http

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.Environment
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import dev.drop.receiver.DropProtocol
import dev.drop.receiver.ui.MainActivity
import fi.iki.elonen.NanoHTTPD
import java.io.File
import java.io.FileOutputStream
import java.net.Inet4Address
import java.net.NetworkInterface

/**
 * HTTP File Server Service
 *
 * Lightweight HTTP server for receiving file uploads from Drop senders.
 */
class FileServerService : Service() {

    companion object {
        private const val TAG = "FileServerService"
        private const val NOTIFICATION_ID = 1002
        private const val CHANNEL_ID = "drop_http_channel"

        const val ACTION_START_SERVER = "dev.drop.receiver.START_SERVER"
        const val ACTION_STOP_SERVER = "dev.drop.receiver.STOP_SERVER"
    }

    private val binder = LocalBinder()
    private var httpServer: DropHttpServer? = null

    var onFileReceived: ((String, Long) -> Unit)? = null
    var onProgress: ((String, Long, Long) -> Unit)? = null

    inner class LocalBinder : Binder() {
        fun getService(): FileServerService = this@FileServerService
    }

    override fun onBind(intent: Intent): IBinder = binder

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START_SERVER -> {
                startForeground(NOTIFICATION_ID, createNotification("Ready to receive files"))
                startServer()
            }
            ACTION_STOP_SERVER -> {
                stopServer()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        stopServer()
        super.onDestroy()
    }

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Drop File Transfer",
            NotificationManager.IMPORTANCE_LOW
        ).apply {
            description = "Shows file transfer progress"
        }

        val notificationManager = getSystemService(NotificationManager::class.java)
        notificationManager.createNotificationChannel(channel)
    }

    private fun createNotification(text: String): Notification {
        val intent = Intent(this, MainActivity::class.java)
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_IMMUTABLE
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Drop Transfer")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .build()
    }

    fun startServer(port: Int = DropProtocol.DEFAULT_HTTP_PORT) {
        if (httpServer != null) {
            Log.d(TAG, "Server already running")
            return
        }

        try {
            httpServer = DropHttpServer(port, getSaveDirectory()).apply {
                onFileReceived = { filename, size ->
                    this@FileServerService.onFileReceived?.invoke(filename, size)
                }
                onProgress = { filename, received, total ->
                    this@FileServerService.onProgress?.invoke(filename, received, total)
                }
                start(NanoHTTPD.SOCKET_READ_TIMEOUT, false)
            }
            Log.d(TAG, "HTTP server started on port $port")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start HTTP server", e)
        }
    }

    fun stopServer() {
        httpServer?.stop()
        httpServer = null
        Log.d(TAG, "HTTP server stopped")
    }

    fun getServerPort(): Int = httpServer?.listeningPort ?: DropProtocol.DEFAULT_HTTP_PORT

    private fun getSaveDirectory(): File {
        val downloadDir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
        val dropDir = File(downloadDir, "Drop")
        if (!dropDir.exists()) {
            dropDir.mkdirs()
        }
        return dropDir
    }

    /**
     * Get the local IP address
     */
    fun getLocalIpAddress(): String? {
        try {
            val interfaces = NetworkInterface.getNetworkInterfaces()
            while (interfaces.hasMoreElements()) {
                val networkInterface = interfaces.nextElement()
                val addresses = networkInterface.inetAddresses

                while (addresses.hasMoreElements()) {
                    val address = addresses.nextElement()
                    if (!address.isLoopbackAddress && address is Inet4Address) {
                        return address.hostAddress
                    }
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to get local IP", e)
        }
        return null
    }
}

/**
 * NanoHTTPD-based HTTP server for file uploads
 */
class DropHttpServer(
    port: Int,
    private val saveDirectory: File
) : NanoHTTPD(port) {

    companion object {
        private const val TAG = "DropHttpServer"
    }

    var onFileReceived: ((String, Long) -> Unit)? = null
    var onProgress: ((String, Long, Long) -> Unit)? = null

    override fun serve(session: IHTTPSession): Response {
        Log.d(TAG, "Request: ${session.method} ${session.uri}")

        return when {
            session.method == Method.OPTIONS -> handleCors()
            session.method == Method.GET && (session.uri == "/" || session.uri == "/health") -> handleHealthCheck()
            session.method == Method.POST && session.uri == "/upload" -> handleUpload(session)
            else -> newFixedLengthResponse(Response.Status.NOT_FOUND, MIME_PLAINTEXT, "Not Found")
        }
    }

    private fun handleCors(): Response {
        return newFixedLengthResponse(Response.Status.NO_CONTENT, MIME_PLAINTEXT, "").apply {
            addHeader("Access-Control-Allow-Origin", "*")
            addHeader("Access-Control-Allow-Methods", "POST, GET, OPTIONS")
            addHeader("Access-Control-Allow-Headers", "Content-Type")
            addHeader("Access-Control-Max-Age", "86400")
        }
    }

    private fun handleHealthCheck(): Response {
        return newFixedLengthResponse(Response.Status.OK, MIME_PLAINTEXT, "Drop Receiver Ready").apply {
            addHeader("Access-Control-Allow-Origin", "*")
        }
    }

    private fun handleUpload(session: IHTTPSession): Response {
        try {
            // Parse multipart data
            val files = HashMap<String, String>()
            session.parseBody(files)

            Log.d(TAG, "Received files map: $files")
            Log.d(TAG, "Parameters: ${session.parameters}")

            // NanoHTTPD stores uploaded files in temp locations
            val tempFilePath = files["file"]
            if (tempFilePath == null) {
                Log.e(TAG, "No file in upload")
                return errorResponse("No file received")
            }

            // Get the original filename from the parameter
            val filename = session.parameters["file"]?.firstOrNull()
                ?: "received_file_${System.currentTimeMillis()}"

            val tempFile = File(tempFilePath)
            val saveFile = getUniqueFile(filename)

            // Copy from temp to final location
            tempFile.inputStream().use { input ->
                FileOutputStream(saveFile).use { output ->
                    val buffer = ByteArray(65536)
                    var bytesRead: Long = 0
                    val totalBytes = tempFile.length()

                    var count: Int
                    while (input.read(buffer).also { count = it } != -1) {
                        output.write(buffer, 0, count)
                        bytesRead += count
                        onProgress?.invoke(filename, bytesRead, totalBytes)
                    }
                }
            }

            // Delete temp file
            tempFile.delete()

            val size = saveFile.length()
            Log.d(TAG, "Saved file: ${saveFile.absolutePath} ($size bytes)")

            onFileReceived?.invoke(saveFile.name, size)

            val responseJson = """{"success":true,"filename":"${saveFile.name}","size":$size}"""

            return newFixedLengthResponse(Response.Status.OK, "application/json", responseJson).apply {
                addHeader("Access-Control-Allow-Origin", "*")
            }

        } catch (e: Exception) {
            Log.e(TAG, "Upload failed", e)
            return errorResponse(e.message ?: "Upload failed")
        }
    }

    private fun getUniqueFile(filename: String): File {
        val sanitized = sanitizeFilename(filename)
        var file = File(saveDirectory, sanitized)

        if (!file.exists()) {
            return file
        }

        // File exists, find unique name
        val dotIndex = sanitized.lastIndexOf('.')
        val name = if (dotIndex > 0) sanitized.substring(0, dotIndex) else sanitized
        val ext = if (dotIndex > 0) sanitized.substring(dotIndex) else ""

        var counter = 1
        while (file.exists() && counter < 1000) {
            file = File(saveDirectory, "$name ($counter)$ext")
            counter++
        }

        return file
    }

    private fun sanitizeFilename(filename: String): String {
        return filename
            .replace(Regex("[/\\\\:*?\"<>|]"), "_")
            .trim()
            .ifEmpty { "unnamed_file" }
    }

    private fun errorResponse(message: String): Response {
        val json = """{"success":false,"error":"$message"}"""
        return newFixedLengthResponse(Response.Status.BAD_REQUEST, "application/json", json).apply {
            addHeader("Access-Control-Allow-Origin", "*")
        }
    }
}
