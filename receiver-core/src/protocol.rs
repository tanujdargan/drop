//! Drop Protocol Definitions
//!
//! This module defines the message types and constants used in the Drop protocol.

use serde::{Deserialize, Serialize};

// BLE Service and Characteristic UUIDs
// Using a custom UUID range based on 0000FE50 (within Bluetooth SIG member range)
pub const DROP_SERVICE_UUID: &str = "0000fe50-0000-1000-8000-00805f9b34fb";
pub const CHAR_DEVICE_INFO_UUID: &str = "0000fe51-0000-1000-8000-00805f9b34fb";
pub const CHAR_TRANSFER_REQUEST_UUID: &str = "0000fe52-0000-1000-8000-00805f9b34fb";
pub const CHAR_TRANSFER_RESPONSE_UUID: &str = "0000fe53-0000-1000-8000-00805f9b34fb";
pub const CHAR_TRANSFER_STATUS_UUID: &str = "0000fe54-0000-1000-8000-00805f9b34fb";

/// Device types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Phone,
    Tablet,
    Laptop,
    Desktop,
}

impl DeviceType {
    pub fn to_byte(&self) -> u8 {
        match self {
            DeviceType::Phone => 0,
            DeviceType::Tablet => 1,
            DeviceType::Laptop => 2,
            DeviceType::Desktop => 3,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => DeviceType::Phone,
            1 => DeviceType::Tablet,
            2 => DeviceType::Laptop,
            _ => DeviceType::Desktop,
        }
    }
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceType::Phone => write!(f, "Phone"),
            DeviceType::Tablet => write!(f, "Tablet"),
            DeviceType::Laptop => write!(f, "Laptop"),
            DeviceType::Desktop => write!(f, "Desktop"),
        }
    }
}

/// Device information advertised over BLE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: DeviceType,
    pub version: u8,
}

impl DeviceInfo {
    pub fn new(name: String, device_type: DeviceType) -> Self {
        Self {
            name,
            device_type,
            version: 1,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

/// File information in a transfer request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub mime_type: String,
}

/// Transfer request sent by the sender
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequest {
    #[serde(rename = "type")]
    pub message_type: String, // "transfer_request"
    pub sender: String,
    pub files: Vec<FileInfo>,
    #[serde(rename = "totalSize")]
    pub total_size: u64,
    #[serde(default)]
    pub session_id: String,
}

impl TransferRequest {
    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn formatted_size(&self) -> String {
        format_size(self.total_size)
    }
}

/// Transfer response sent by the receiver
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResponse {
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl TransferResponse {
    pub fn accept(ip: String, port: u16) -> Self {
        Self {
            accepted: true,
            reason: None,
            ip: Some(ip),
            port: Some(port),
        }
    }

    pub fn reject(reason: String) -> Self {
        Self {
            accepted: false,
            reason: Some(reason),
            ip: None,
            port: None,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Transfer status updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferStatus {
    #[serde(rename = "type")]
    pub status_type: String, // "receiving", "complete", "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>, // 0.0 - 1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TransferStatus {
    pub fn receiving(progress: f32) -> Self {
        Self {
            status_type: "receiving".to_string(),
            progress: Some(progress),
            error: None,
        }
    }

    pub fn complete() -> Self {
        Self {
            status_type: "complete".to_string(),
            progress: Some(1.0),
            error: None,
        }
    }

    pub fn error(msg: String) -> Self {
        Self {
            status_type: "error".to_string(),
            progress: None,
            error: Some(msg),
        }
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

/// Format file size in human-readable form
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_serialization() {
        let info = DeviceInfo::new("Test Device".to_string(), DeviceType::Desktop);
        let json = info.to_json();
        assert!(json.contains("Test Device"));
        assert!(json.contains("desktop"));

        let parsed = DeviceInfo::from_json(&json).unwrap();
        assert_eq!(parsed.name, "Test Device");
        assert_eq!(parsed.device_type, DeviceType::Desktop);
    }

    #[test]
    fn test_transfer_request_parsing() {
        let json = r#"{
            "type": "transfer_request",
            "sender": "Web Browser",
            "files": [
                {"name": "photo.jpg", "size": 1024000, "type": "image/jpeg"}
            ],
            "totalSize": 1024000
        }"#;

        let request = TransferRequest::from_json(json).unwrap();
        assert_eq!(request.sender, "Web Browser");
        assert_eq!(request.files.len(), 1);
        assert_eq!(request.files[0].name, "photo.jpg");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
    }
}
