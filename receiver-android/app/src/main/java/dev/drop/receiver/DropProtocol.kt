package dev.drop.receiver

import java.util.UUID

/**
 * Drop Protocol Constants and Data Classes
 */
object DropProtocol {
    // BLE Service and Characteristic UUIDs
    val DROP_SERVICE_UUID: UUID = UUID.fromString("0000fe50-0000-1000-8000-00805f9b34fb")
    val CHAR_DEVICE_INFO_UUID: UUID = UUID.fromString("0000fe51-0000-1000-8000-00805f9b34fb")
    val CHAR_TRANSFER_REQUEST_UUID: UUID = UUID.fromString("0000fe52-0000-1000-8000-00805f9b34fb")
    val CHAR_TRANSFER_RESPONSE_UUID: UUID = UUID.fromString("0000fe53-0000-1000-8000-00805f9b34fb")
    val CHAR_TRANSFER_STATUS_UUID: UUID = UUID.fromString("0000fe54-0000-1000-8000-00805f9b34fb")

    // Client Characteristic Configuration Descriptor (for notifications)
    val CCCD_UUID: UUID = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")

    // Default HTTP port for file transfers
    const val DEFAULT_HTTP_PORT = 53317
}

/**
 * Device types
 */
enum class DeviceType(val value: Int) {
    PHONE(0),
    TABLET(1),
    LAPTOP(2),
    DESKTOP(3);

    companion object {
        fun fromValue(value: Int): DeviceType = entries.find { it.value == value } ?: PHONE
    }
}

/**
 * Device information
 */
data class DeviceInfo(
    val name: String,
    val type: DeviceType,
    val version: Int = 1
)

/**
 * File information in a transfer request
 */
data class FileInfo(
    val name: String,
    val size: Long,
    val type: String
)

/**
 * Transfer request from sender
 */
data class TransferRequest(
    val type: String = "transfer_request",
    val sender: String,
    val files: List<FileInfo>,
    val totalSize: Long,
    val sessionId: String = UUID.randomUUID().toString()
) {
    val fileCount: Int get() = files.size

    val formattedSize: String get() = formatFileSize(totalSize)

    companion object {
        fun formatFileSize(bytes: Long): String {
            return when {
                bytes >= 1_073_741_824 -> String.format("%.1f GB", bytes / 1_073_741_824.0)
                bytes >= 1_048_576 -> String.format("%.1f MB", bytes / 1_048_576.0)
                bytes >= 1024 -> String.format("%.1f KB", bytes / 1024.0)
                else -> "$bytes B"
            }
        }
    }
}

/**
 * Transfer response to sender
 */
data class TransferResponse(
    val accepted: Boolean,
    val reason: String? = null,
    val ip: String? = null,
    val port: Int? = null
) {
    companion object {
        fun accept(ip: String, port: Int) = TransferResponse(
            accepted = true,
            ip = ip,
            port = port
        )

        fun reject(reason: String) = TransferResponse(
            accepted = false,
            reason = reason
        )
    }
}

/**
 * Transfer status update
 */
data class TransferStatus(
    val type: String, // "receiving", "complete", "error"
    val progress: Float? = null,
    val error: String? = null
) {
    companion object {
        fun receiving(progress: Float) = TransferStatus("receiving", progress)
        fun complete() = TransferStatus("complete", 1.0f)
        fun error(message: String) = TransferStatus("error", error = message)
    }
}
