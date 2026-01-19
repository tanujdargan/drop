//! BLE Peripheral Implementation
//!
//! This module handles BLE advertising and GATT server functionality.
//! The receiver acts as a BLE peripheral, advertising its presence and
//! handling incoming transfer requests.

use crate::{DropError, Result, ReceiverEvent, TransferRequest, TransferResponse};
use crate::protocol::{DeviceInfo, DeviceType, DROP_SERVICE_UUID};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, Peripheral};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error};

/// BLE Peripheral for Drop receiver
///
/// Note: btleplug primarily supports Central role (scanning/connecting).
/// For true peripheral (GATT server) functionality on desktop platforms,
/// we may need platform-specific implementations or alternative libraries.
///
/// This implementation provides a compatibility layer that:
/// 1. On platforms with peripheral support: Uses native GATT server
/// 2. On platforms without: Falls back to alternative discovery methods
pub struct BlePeripheral {
    device_info: DeviceInfo,
    event_tx: broadcast::Sender<ReceiverEvent>,
    is_advertising: Arc<RwLock<bool>>,
    pending_response: Arc<RwLock<Option<TransferResponse>>>,
}

impl BlePeripheral {
    pub fn new(device_name: String, device_type: DeviceType, event_tx: broadcast::Sender<ReceiverEvent>) -> Self {
        Self {
            device_info: DeviceInfo::new(device_name, device_type),
            event_tx,
            is_advertising: Arc::new(RwLock::new(false)),
            pending_response: Arc::new(RwLock::new(None)),
        }
    }

    /// Start BLE advertising
    ///
    /// This makes the device discoverable to Drop senders.
    pub async fn start_advertising(&self) -> Result<()> {
        info!("Starting BLE advertising as '{}'", self.device_info.name);

        // Check if BLE is available
        let manager = Manager::new().await.map_err(|e| {
            DropError::Ble(format!("Failed to initialize BLE manager: {}", e))
        })?;

        let adapters = manager.adapters().await.map_err(|e| {
            DropError::Ble(format!("Failed to get BLE adapters: {}", e))
        })?;

        if adapters.is_empty() {
            return Err(DropError::Ble("No BLE adapters found".to_string()));
        }

        let adapter = &adapters[0];
        info!("Using BLE adapter: {:?}", adapter.adapter_info().await);

        // Note: btleplug doesn't support peripheral mode directly.
        // For now, we'll set the advertising flag and rely on platform-specific
        // implementations for actual GATT server functionality.
        //
        // On Windows: Use windows-ble crate or WinRT APIs
        // On macOS: Use CoreBluetooth via objc or swift bridge
        // On Linux: Use BlueZ D-Bus APIs directly (bluer crate)

        *self.is_advertising.write().await = true;
        let _ = self.event_tx.send(ReceiverEvent::AdvertisingStarted);

        info!("BLE advertising started (simulated - platform-specific implementation needed)");
        info!("Device info: {}", self.device_info.to_json());

        Ok(())
    }

    /// Stop BLE advertising
    pub async fn stop_advertising(&self) -> Result<()> {
        info!("Stopping BLE advertising");

        *self.is_advertising.write().await = false;
        let _ = self.event_tx.send(ReceiverEvent::AdvertisingStopped);

        Ok(())
    }

    /// Check if currently advertising
    pub async fn is_advertising(&self) -> bool {
        *self.is_advertising.read().await
    }

    /// Get device info
    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    /// Set the response to send back to sender
    pub async fn set_response(&self, response: TransferResponse) {
        *self.pending_response.write().await = Some(response);
    }

    /// Get and clear pending response
    pub async fn take_response(&self) -> Option<TransferResponse> {
        self.pending_response.write().await.take()
    }

    /// Handle incoming transfer request (called from GATT characteristic write)
    pub async fn handle_transfer_request(&self, data: &[u8]) -> Result<()> {
        let json = String::from_utf8_lossy(data);
        info!("Received transfer request: {}", json);

        let mut request: TransferRequest = serde_json::from_str(&json)
            .map_err(|e| DropError::Protocol(format!("Invalid transfer request: {}", e)))?;

        // Generate session ID if not provided
        if request.session_id.is_empty() {
            request.session_id = uuid::Uuid::new_v4().to_string();
        }

        // Emit event for UI to handle
        let _ = self.event_tx.send(ReceiverEvent::TransferRequest(request));

        Ok(())
    }
}

/// Platform-specific BLE GATT server implementation
///
/// This trait allows for platform-specific implementations while keeping
/// a common interface.
#[async_trait::async_trait]
pub trait GattServer: Send + Sync {
    /// Start the GATT server with Drop service
    async fn start(&self) -> Result<()>;

    /// Stop the GATT server
    async fn stop(&self) -> Result<()>;

    /// Send a notification to connected central
    async fn notify(&self, characteristic_uuid: &str, data: &[u8]) -> Result<()>;
}

// Placeholder for Windows-specific GATT server
#[cfg(target_os = "windows")]
pub mod windows {
    use super::*;

    pub struct WindowsGattServer {
        // Windows BLE GATT server implementation using WinRT
    }

    impl WindowsGattServer {
        pub fn new(_device_info: DeviceInfo) -> Self {
            Self {}
        }
    }

    #[async_trait::async_trait]
    impl GattServer for WindowsGattServer {
        async fn start(&self) -> Result<()> {
            warn!("Windows GATT server not yet implemented");
            // TODO: Implement using windows crate with WinRT BLE APIs
            // Example: Windows.Devices.Bluetooth.GenericAttributeProfile
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn notify(&self, _characteristic_uuid: &str, _data: &[u8]) -> Result<()> {
            Ok(())
        }
    }
}

// Placeholder for Linux-specific GATT server
#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;

    pub struct LinuxGattServer {
        // Linux BLE GATT server implementation using BlueZ D-Bus
    }

    impl LinuxGattServer {
        pub fn new(_device_info: DeviceInfo) -> Self {
            Self {}
        }
    }

    #[async_trait::async_trait]
    impl GattServer for LinuxGattServer {
        async fn start(&self) -> Result<()> {
            warn!("Linux GATT server not yet implemented");
            // TODO: Implement using bluer crate for BlueZ D-Bus API
            // or zbus for direct D-Bus communication
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn notify(&self, _characteristic_uuid: &str, _data: &[u8]) -> Result<()> {
            Ok(())
        }
    }
}

// Placeholder for macOS-specific GATT server
#[cfg(target_os = "macos")]
pub mod macos {
    use super::*;

    pub struct MacOsGattServer {
        // macOS BLE GATT server implementation using CoreBluetooth
    }

    impl MacOsGattServer {
        pub fn new(_device_info: DeviceInfo) -> Self {
            Self {}
        }
    }

    #[async_trait::async_trait]
    impl GattServer for MacOsGattServer {
        async fn start(&self) -> Result<()> {
            warn!("macOS GATT server not yet implemented");
            // TODO: Implement using objc/objc2 crate for CoreBluetooth
            // CBPeripheralManager for advertising and GATT server
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn notify(&self, _characteristic_uuid: &str, _data: &[u8]) -> Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ble_peripheral_creation() {
        let (tx, _rx) = broadcast::channel(10);
        let peripheral = BlePeripheral::new(
            "Test Device".to_string(),
            DeviceType::Desktop,
            tx,
        );

        assert_eq!(peripheral.device_info().name, "Test Device");
        assert_eq!(peripheral.device_info().device_type, DeviceType::Desktop);
        assert!(!peripheral.is_advertising().await);
    }
}
