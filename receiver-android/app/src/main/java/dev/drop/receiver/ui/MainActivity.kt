package dev.drop.receiver.ui

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.util.Log
import android.view.View
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.google.gson.Gson
import dev.drop.receiver.*
import dev.drop.receiver.ble.DropGattService
import dev.drop.receiver.databinding.ActivityMainBinding
import dev.drop.receiver.http.FileServerService

class MainActivity : AppCompatActivity() {

    companion object {
        private const val TAG = "MainActivity"
        private const val REQUEST_PERMISSIONS = 100
    }

    private lateinit var binding: ActivityMainBinding
    private val gson = Gson()

    private var gattService: DropGattService? = null
    private var fileServerService: FileServerService? = null
    private var isGattBound = false
    private var isFileBound = false

    private var pendingRequest: TransferRequest? = null

    // Required permissions
    private val requiredPermissions = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        arrayOf(
            Manifest.permission.BLUETOOTH_SCAN,
            Manifest.permission.BLUETOOTH_ADVERTISE,
            Manifest.permission.BLUETOOTH_CONNECT,
            Manifest.permission.POST_NOTIFICATIONS
        )
    } else {
        arrayOf(
            Manifest.permission.BLUETOOTH,
            Manifest.permission.BLUETOOTH_ADMIN,
            Manifest.permission.ACCESS_FINE_LOCATION
        )
    }

    private val enableBluetoothLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == RESULT_OK) {
            startServices()
        } else {
            showError("Bluetooth is required for Drop to work")
        }
    }

    private val gattServiceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
            val binder = service as DropGattService.LocalBinder
            gattService = binder.getService().apply {
                onTransferRequest = { request -> handleTransferRequest(request) }
                onConnectionStateChanged = { connected -> updateConnectionState(connected) }
            }
            isGattBound = true
            Log.d(TAG, "GATT service connected")
            updateUI()
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            gattService = null
            isGattBound = false
            Log.d(TAG, "GATT service disconnected")
            updateUI()
        }
    }

    private val fileServiceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
            val binder = service as FileServerService.LocalBinder
            fileServerService = binder.getService().apply {
                onFileReceived = { filename, size -> handleFileReceived(filename, size) }
                onProgress = { filename, received, total -> updateProgress(filename, received, total) }
            }
            isFileBound = true
            Log.d(TAG, "File server service connected")
            updateUI()
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            fileServerService = null
            isFileBound = false
            Log.d(TAG, "File server service disconnected")
            updateUI()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        setupUI()
        checkPermissions()
    }

    override fun onDestroy() {
        super.onDestroy()
        if (isGattBound) {
            unbindService(gattServiceConnection)
        }
        if (isFileBound) {
            unbindService(fileServiceConnection)
        }
    }

    private fun setupUI() {
        binding.apply {
            btnToggle.setOnClickListener { toggleReceiver() }

            btnAccept.setOnClickListener {
                pendingRequest?.let { acceptTransfer(it) }
            }

            btnReject.setOnClickListener {
                pendingRequest?.let { rejectTransfer(it) }
            }
        }

        updateUI()
    }

    private fun checkPermissions() {
        val missingPermissions = requiredPermissions.filter {
            ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED
        }

        if (missingPermissions.isNotEmpty()) {
            ActivityCompat.requestPermissions(this, missingPermissions.toTypedArray(), REQUEST_PERMISSIONS)
        } else {
            checkBluetoothEnabled()
        }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)

        if (requestCode == REQUEST_PERMISSIONS) {
            if (grantResults.all { it == PackageManager.PERMISSION_GRANTED }) {
                checkBluetoothEnabled()
            } else {
                showError("Permissions are required for Drop to work")
            }
        }
    }

    private fun checkBluetoothEnabled() {
        val bluetoothManager = getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
        val bluetoothAdapter = bluetoothManager.adapter

        if (bluetoothAdapter == null) {
            showError("Bluetooth is not available on this device")
            return
        }

        if (!bluetoothAdapter.isEnabled) {
            val enableIntent = Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE)
            enableBluetoothLauncher.launch(enableIntent)
        } else {
            startServices()
        }
    }

    private fun startServices() {
        // Start and bind GATT service
        Intent(this, DropGattService::class.java).also { intent ->
            intent.action = DropGattService.ACTION_START_ADVERTISING
            startForegroundService(intent)
            bindService(intent, gattServiceConnection, Context.BIND_AUTO_CREATE)
        }

        // Start and bind file server service
        Intent(this, FileServerService::class.java).also { intent ->
            intent.action = FileServerService.ACTION_START_SERVER
            startForegroundService(intent)
            bindService(intent, fileServiceConnection, Context.BIND_AUTO_CREATE)
        }
    }

    private fun stopServices() {
        Intent(this, DropGattService::class.java).also { intent ->
            intent.action = DropGattService.ACTION_STOP_ADVERTISING
            startService(intent)
        }

        Intent(this, FileServerService::class.java).also { intent ->
            intent.action = FileServerService.ACTION_STOP_SERVER
            startService(intent)
        }
    }

    private fun toggleReceiver() {
        if (isGattBound && isFileBound) {
            stopServices()
        } else {
            startServices()
        }
    }

    private fun handleTransferRequest(request: TransferRequest) {
        runOnUiThread {
            Log.d(TAG, "Transfer request: ${request.sender} - ${request.fileCount} files (${request.formattedSize})")

            pendingRequest = request

            binding.apply {
                transferRequestCard.visibility = View.VISIBLE
                txtSender.text = "From: ${request.sender}"
                txtFiles.text = "${request.fileCount} file${if (request.fileCount > 1) "s" else ""}"
                txtSize.text = request.formattedSize

                // List files
                val fileNames = request.files.joinToString("\n") { "• ${it.name}" }
                txtFileList.text = fileNames
            }
        }
    }

    private fun acceptTransfer(request: TransferRequest) {
        val ip = fileServerService?.getLocalIpAddress()
        val port = fileServerService?.getServerPort() ?: DropProtocol.DEFAULT_HTTP_PORT

        if (ip == null) {
            showError("Could not determine local IP address")
            return
        }

        val response = TransferResponse.accept(ip, port)

        gattService?.sendResponse(response)

        binding.apply {
            transferRequestCard.visibility = View.GONE
            transferProgressCard.visibility = View.VISIBLE
            txtTransferStatus.text = "Waiting for files..."
            progressBar.progress = 0
        }

        pendingRequest = null
        Log.d(TAG, "Accepted transfer, listening on $ip:$port")
    }

    private fun rejectTransfer(request: TransferRequest) {
        val response = TransferResponse.reject("User rejected")

        gattService?.sendResponse(response)

        binding.transferRequestCard.visibility = View.GONE
        pendingRequest = null

        Log.d(TAG, "Rejected transfer")
    }

    private fun handleFileReceived(filename: String, size: Long) {
        runOnUiThread {
            binding.apply {
                transferProgressCard.visibility = View.GONE
                txtStatus.text = "Received: $filename"
            }

            Toast.makeText(
                this,
                "File received: $filename (${TransferRequest.formatFileSize(size)})",
                Toast.LENGTH_LONG
            ).show()
        }
    }

    private fun updateProgress(filename: String, received: Long, total: Long) {
        runOnUiThread {
            val percent = if (total > 0) (received * 100 / total).toInt() else 0

            binding.apply {
                txtTransferStatus.text = "Receiving: $filename"
                progressBar.progress = percent
                txtProgress.text = "${TransferRequest.formatFileSize(received)} / ${TransferRequest.formatFileSize(total)}"
            }
        }
    }

    private fun updateConnectionState(connected: Boolean) {
        runOnUiThread {
            binding.txtConnectionStatus.text = if (connected) "Device connected" else ""
        }
    }

    private fun updateUI() {
        runOnUiThread {
            val isActive = isGattBound && isFileBound

            binding.apply {
                btnToggle.text = if (isActive) "Stop Receiver" else "Start Receiver"

                if (isActive) {
                    txtStatus.text = "Ready to receive files"
                    val ip = fileServerService?.getLocalIpAddress() ?: "Unknown"
                    txtIpAddress.text = "IP: $ip"
                    txtIpAddress.visibility = View.VISIBLE
                    statusIndicator.setBackgroundColor(getColor(android.R.color.holo_green_light))
                } else {
                    txtStatus.text = "Receiver stopped"
                    txtIpAddress.visibility = View.GONE
                    statusIndicator.setBackgroundColor(getColor(android.R.color.darker_gray))
                }
            }
        }
    }

    private fun showError(message: String) {
        AlertDialog.Builder(this)
            .setTitle("Error")
            .setMessage(message)
            .setPositiveButton("OK", null)
            .show()
    }
}
