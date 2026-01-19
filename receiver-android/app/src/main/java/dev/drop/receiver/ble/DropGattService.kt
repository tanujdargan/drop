package dev.drop.receiver.ble

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.bluetooth.*
import android.bluetooth.le.AdvertiseCallback
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertiseSettings
import android.bluetooth.le.BluetoothLeAdvertiser
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.os.ParcelUuid
import android.util.Log
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import com.google.gson.Gson
import dev.drop.receiver.*
import dev.drop.receiver.ui.MainActivity
import java.util.*

/**
 * BLE GATT Server Service for Drop
 *
 * This service handles:
 * 1. BLE advertising to make the device discoverable
 * 2. GATT server with Drop service and characteristics
 * 3. Handling incoming transfer requests
 */
class DropGattService : Service() {

    companion object {
        private const val TAG = "DropGattService"
        private const val NOTIFICATION_ID = 1001
        private const val CHANNEL_ID = "drop_ble_channel"

        // Actions
        const val ACTION_START_ADVERTISING = "dev.drop.receiver.START_ADVERTISING"
        const val ACTION_STOP_ADVERTISING = "dev.drop.receiver.STOP_ADVERTISING"
        const val ACTION_SEND_RESPONSE = "dev.drop.receiver.SEND_RESPONSE"

        // Extras
        const val EXTRA_RESPONSE_JSON = "response_json"
    }

    private val binder = LocalBinder()
    private val gson = Gson()

    private var bluetoothManager: BluetoothManager? = null
    private var bluetoothAdapter: BluetoothAdapter? = null
    private var bluetoothLeAdvertiser: BluetoothLeAdvertiser? = null
    private var gattServer: BluetoothGattServer? = null

    private var isAdvertising = false
    private var connectedDevice: BluetoothDevice? = null

    // Characteristics
    private var deviceInfoCharacteristic: BluetoothGattCharacteristic? = null
    private var transferRequestCharacteristic: BluetoothGattCharacteristic? = null
    private var transferResponseCharacteristic: BluetoothGattCharacteristic? = null
    private var transferStatusCharacteristic: BluetoothGattCharacteristic? = null

    // Callbacks
    var onTransferRequest: ((TransferRequest) -> Unit)? = null
    var onConnectionStateChanged: ((Boolean) -> Unit)? = null

    // Pending response to send
    private var pendingResponse: TransferResponse? = null

    inner class LocalBinder : Binder() {
        fun getService(): DropGattService = this@DropGattService
    }

    override fun onBind(intent: Intent): IBinder = binder

    override fun onCreate() {
        super.onCreate()
        Log.d(TAG, "Service created")
        initializeBluetooth()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Log.d(TAG, "onStartCommand: ${intent?.action}")

        when (intent?.action) {
            ACTION_START_ADVERTISING -> {
                startForeground(NOTIFICATION_ID, createNotification("Drop Receiver Active"))
                startAdvertising()
            }
            ACTION_STOP_ADVERTISING -> {
                stopAdvertising()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
            ACTION_SEND_RESPONSE -> {
                intent.getStringExtra(EXTRA_RESPONSE_JSON)?.let { json ->
                    val response = gson.fromJson(json, TransferResponse::class.java)
                    sendResponse(response)
                }
            }
        }

        return START_STICKY
    }

    override fun onDestroy() {
        Log.d(TAG, "Service destroyed")
        stopAdvertising()
        closeGattServer()
        super.onDestroy()
    }

    private fun initializeBluetooth() {
        bluetoothManager = getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
        bluetoothAdapter = bluetoothManager?.adapter
        bluetoothLeAdvertiser = bluetoothAdapter?.bluetoothLeAdvertiser

        if (bluetoothLeAdvertiser == null) {
            Log.e(TAG, "BLE advertising not supported")
        }
    }

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Drop Receiver",
            NotificationManager.IMPORTANCE_LOW
        ).apply {
            description = "Shows when Drop receiver is active"
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
            .setContentTitle("Drop")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .build()
    }

    fun startAdvertising() {
        if (isAdvertising) {
            Log.d(TAG, "Already advertising")
            return
        }

        if (!hasBluetoothPermissions()) {
            Log.e(TAG, "Missing Bluetooth permissions")
            return
        }

        // Start GATT server first
        startGattServer()

        // Then start advertising
        val settings = AdvertiseSettings.Builder()
            .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_LOW_LATENCY)
            .setConnectable(true)
            .setTimeout(0) // Advertise indefinitely
            .setTxPowerLevel(AdvertiseSettings.ADVERTISE_TX_POWER_HIGH)
            .build()

        val data = AdvertiseData.Builder()
            .setIncludeDeviceName(true)
            .addServiceUuid(ParcelUuid(DropProtocol.DROP_SERVICE_UUID))
            .build()

        val scanResponse = AdvertiseData.Builder()
            .setIncludeDeviceName(false)
            .build()

        try {
            if (ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_ADVERTISE)
                    == PackageManager.PERMISSION_GRANTED) {
                bluetoothLeAdvertiser?.startAdvertising(settings, data, scanResponse, advertiseCallback)
                Log.d(TAG, "Started advertising")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start advertising", e)
        }
    }

    fun stopAdvertising() {
        if (!isAdvertising) return

        try {
            if (ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_ADVERTISE)
                    == PackageManager.PERMISSION_GRANTED) {
                bluetoothLeAdvertiser?.stopAdvertising(advertiseCallback)
            }
            isAdvertising = false
            Log.d(TAG, "Stopped advertising")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to stop advertising", e)
        }
    }

    private fun startGattServer() {
        if (gattServer != null) return

        if (ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_CONNECT)
                != PackageManager.PERMISSION_GRANTED) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission")
            return
        }

        gattServer = bluetoothManager?.openGattServer(this, gattServerCallback)

        // Create the Drop service
        val service = BluetoothGattService(
            DropProtocol.DROP_SERVICE_UUID,
            BluetoothGattService.SERVICE_TYPE_PRIMARY
        )

        // Device Info characteristic (Read)
        deviceInfoCharacteristic = BluetoothGattCharacteristic(
            DropProtocol.CHAR_DEVICE_INFO_UUID,
            BluetoothGattCharacteristic.PROPERTY_READ,
            BluetoothGattCharacteristic.PERMISSION_READ
        ).apply {
            val deviceInfo = DeviceInfo(
                name = Build.MODEL,
                type = if (resources.configuration.smallestScreenWidthDp >= 600)
                    DeviceType.TABLET else DeviceType.PHONE
            )
            value = gson.toJson(deviceInfo).toByteArray()
        }
        service.addCharacteristic(deviceInfoCharacteristic)

        // Transfer Request characteristic (Write)
        transferRequestCharacteristic = BluetoothGattCharacteristic(
            DropProtocol.CHAR_TRANSFER_REQUEST_UUID,
            BluetoothGattCharacteristic.PROPERTY_WRITE,
            BluetoothGattCharacteristic.PERMISSION_WRITE
        )
        service.addCharacteristic(transferRequestCharacteristic)

        // Transfer Response characteristic (Notify)
        transferResponseCharacteristic = BluetoothGattCharacteristic(
            DropProtocol.CHAR_TRANSFER_RESPONSE_UUID,
            BluetoothGattCharacteristic.PROPERTY_NOTIFY or BluetoothGattCharacteristic.PROPERTY_READ,
            BluetoothGattCharacteristic.PERMISSION_READ
        ).apply {
            addDescriptor(BluetoothGattDescriptor(
                DropProtocol.CCCD_UUID,
                BluetoothGattDescriptor.PERMISSION_READ or BluetoothGattDescriptor.PERMISSION_WRITE
            ))
        }
        service.addCharacteristic(transferResponseCharacteristic)

        // Transfer Status characteristic (Write + Notify)
        transferStatusCharacteristic = BluetoothGattCharacteristic(
            DropProtocol.CHAR_TRANSFER_STATUS_UUID,
            BluetoothGattCharacteristic.PROPERTY_WRITE or BluetoothGattCharacteristic.PROPERTY_NOTIFY,
            BluetoothGattCharacteristic.PERMISSION_WRITE
        ).apply {
            addDescriptor(BluetoothGattDescriptor(
                DropProtocol.CCCD_UUID,
                BluetoothGattDescriptor.PERMISSION_READ or BluetoothGattDescriptor.PERMISSION_WRITE
            ))
        }
        service.addCharacteristic(transferStatusCharacteristic)

        gattServer?.addService(service)
        Log.d(TAG, "GATT server started with Drop service")
    }

    private fun closeGattServer() {
        if (ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_CONNECT)
                == PackageManager.PERMISSION_GRANTED) {
            gattServer?.close()
        }
        gattServer = null
    }

    fun sendResponse(response: TransferResponse) {
        val device = connectedDevice
        if (device == null) {
            Log.e(TAG, "No connected device to send response to")
            pendingResponse = response
            return
        }

        val characteristic = transferResponseCharacteristic
        if (characteristic == null) {
            Log.e(TAG, "Response characteristic not initialized")
            return
        }

        if (ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_CONNECT)
                != PackageManager.PERMISSION_GRANTED) {
            return
        }

        val json = gson.toJson(response)
        characteristic.value = json.toByteArray()

        val success = gattServer?.notifyCharacteristicChanged(device, characteristic, false)
        Log.d(TAG, "Sent response notification: $json, success: $success")
    }

    private fun hasBluetoothPermissions(): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_ADVERTISE) == PackageManager.PERMISSION_GRANTED &&
            ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED
        } else {
            ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH) == PackageManager.PERMISSION_GRANTED &&
            ActivityCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_ADMIN) == PackageManager.PERMISSION_GRANTED
        }
    }

    // Advertise callback
    private val advertiseCallback = object : AdvertiseCallback() {
        override fun onStartSuccess(settingsInEffect: AdvertiseSettings?) {
            Log.d(TAG, "Advertising started successfully")
            isAdvertising = true
        }

        override fun onStartFailure(errorCode: Int) {
            Log.e(TAG, "Advertising failed with error code: $errorCode")
            isAdvertising = false
        }
    }

    // GATT server callback
    private val gattServerCallback = object : BluetoothGattServerCallback() {

        override fun onConnectionStateChange(device: BluetoothDevice?, status: Int, newState: Int) {
            Log.d(TAG, "Connection state changed: device=${device?.address}, status=$status, newState=$newState")

            when (newState) {
                BluetoothProfile.STATE_CONNECTED -> {
                    connectedDevice = device
                    onConnectionStateChanged?.invoke(true)
                    Log.d(TAG, "Device connected: ${device?.address}")
                }
                BluetoothProfile.STATE_DISCONNECTED -> {
                    connectedDevice = null
                    onConnectionStateChanged?.invoke(false)
                    Log.d(TAG, "Device disconnected")
                }
            }
        }

        override fun onCharacteristicReadRequest(
            device: BluetoothDevice?,
            requestId: Int,
            offset: Int,
            characteristic: BluetoothGattCharacteristic?
        ) {
            Log.d(TAG, "Read request for ${characteristic?.uuid}")

            if (ActivityCompat.checkSelfPermission(this@DropGattService, Manifest.permission.BLUETOOTH_CONNECT)
                    != PackageManager.PERMISSION_GRANTED) {
                return
            }

            when (characteristic?.uuid) {
                DropProtocol.CHAR_DEVICE_INFO_UUID -> {
                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_SUCCESS,
                        offset,
                        characteristic.value
                    )
                }
                DropProtocol.CHAR_TRANSFER_RESPONSE_UUID -> {
                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_SUCCESS,
                        offset,
                        characteristic.value ?: byteArrayOf()
                    )
                }
                else -> {
                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_FAILURE,
                        0,
                        null
                    )
                }
            }
        }

        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice?,
            requestId: Int,
            characteristic: BluetoothGattCharacteristic?,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray?
        ) {
            Log.d(TAG, "Write request for ${characteristic?.uuid}: ${value?.toString(Charsets.UTF_8)}")

            if (ActivityCompat.checkSelfPermission(this@DropGattService, Manifest.permission.BLUETOOTH_CONNECT)
                    != PackageManager.PERMISSION_GRANTED) {
                return
            }

            when (characteristic?.uuid) {
                DropProtocol.CHAR_TRANSFER_REQUEST_UUID -> {
                    // Send success response first
                    if (responseNeeded) {
                        gattServer?.sendResponse(
                            device,
                            requestId,
                            BluetoothGatt.GATT_SUCCESS,
                            0,
                            null
                        )
                    }

                    // Parse and handle the transfer request
                    value?.let { bytes ->
                        try {
                            val json = String(bytes, Charsets.UTF_8)
                            Log.d(TAG, "Received transfer request: $json")

                            val request = gson.fromJson(json, TransferRequest::class.java)
                            onTransferRequest?.invoke(request)
                        } catch (e: Exception) {
                            Log.e(TAG, "Failed to parse transfer request", e)
                        }
                    }
                }
                DropProtocol.CHAR_TRANSFER_STATUS_UUID -> {
                    if (responseNeeded) {
                        gattServer?.sendResponse(
                            device,
                            requestId,
                            BluetoothGatt.GATT_SUCCESS,
                            0,
                            null
                        )
                    }

                    value?.let { bytes ->
                        val json = String(bytes, Charsets.UTF_8)
                        Log.d(TAG, "Received transfer status: $json")
                    }
                }
                else -> {
                    if (responseNeeded) {
                        gattServer?.sendResponse(
                            device,
                            requestId,
                            BluetoothGatt.GATT_FAILURE,
                            0,
                            null
                        )
                    }
                }
            }
        }

        override fun onDescriptorWriteRequest(
            device: BluetoothDevice?,
            requestId: Int,
            descriptor: BluetoothGattDescriptor?,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray?
        ) {
            Log.d(TAG, "Descriptor write request: ${descriptor?.uuid}")

            if (ActivityCompat.checkSelfPermission(this@DropGattService, Manifest.permission.BLUETOOTH_CONNECT)
                    != PackageManager.PERMISSION_GRANTED) {
                return
            }

            // Handle CCCD writes (notification subscription)
            if (descriptor?.uuid == DropProtocol.CCCD_UUID) {
                if (responseNeeded) {
                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_SUCCESS,
                        0,
                        null
                    )
                }
                Log.d(TAG, "Notifications enabled for ${descriptor.characteristic?.uuid}")
            }
        }
    }
}
