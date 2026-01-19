//! Drop Receiver Core Library
//!
//! This library provides the core functionality for Drop file receivers:
//! - BLE peripheral advertising
//! - HTTP file receive server
//! - Protocol handling

pub mod protocol;
pub mod ble;
pub mod http_server;
pub mod storage;

use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use thiserror::Error;

pub use protocol::*;
pub use ble::BlePeripheral;
pub use http_server::FileReceiveServer;
pub use storage::StorageManager;

#[derive(Debug, Error)]
pub enum DropError {
    #[error("BLE error: {0}")]
    Ble(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Storage error: {0}")]
    Storage(String),
}

pub type Result<T> = std::result::Result<T, DropError>;

/// Events emitted by the receiver
#[derive(Debug, Clone)]
pub enum ReceiverEvent {
    /// BLE advertising started
    AdvertisingStarted,
    /// BLE advertising stopped
    AdvertisingStopped,
    /// Transfer request received from sender
    TransferRequest(TransferRequest),
    /// Transfer accepted by user
    TransferAccepted(String), // session_id
    /// Transfer rejected by user
    TransferRejected(String), // session_id
    /// File transfer started
    TransferStarted { session_id: String, file_name: String },
    /// File transfer progress
    TransferProgress { session_id: String, file_name: String, bytes_received: u64, total_bytes: u64 },
    /// File transfer completed
    TransferComplete { session_id: String, file_name: String, save_path: String },
    /// Transfer failed
    TransferFailed { session_id: String, error: String },
    /// All files in session transferred
    SessionComplete { session_id: String },
}

/// Configuration for the Drop receiver
#[derive(Debug, Clone)]
pub struct ReceiverConfig {
    /// Device name to advertise
    pub device_name: String,
    /// Device type (phone, tablet, laptop, desktop)
    pub device_type: DeviceType,
    /// Port for HTTP file receive server
    pub http_port: u16,
    /// Directory to save received files
    pub save_directory: String,
    /// Whether to auto-accept transfers (for testing)
    pub auto_accept: bool,
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        let save_dir = dirs::download_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .to_string_lossy()
            .to_string();

        Self {
            device_name: whoami::devicename(),
            device_type: DeviceType::Desktop,
            http_port: 53317,
            save_directory: save_dir,
            auto_accept: false,
        }
    }
}

/// Main Drop receiver instance
pub struct DropReceiver {
    config: ReceiverConfig,
    event_tx: broadcast::Sender<ReceiverEvent>,
    pending_requests: Arc<RwLock<Vec<TransferRequest>>>,
    active_sessions: Arc<RwLock<std::collections::HashMap<String, TransferSession>>>,
}

#[derive(Debug, Clone)]
pub struct TransferSession {
    pub id: String,
    pub request: TransferRequest,
    pub accepted: bool,
    pub files_received: usize,
    pub total_files: usize,
}

impl DropReceiver {
    pub fn new(config: ReceiverConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            config,
            event_tx,
            pending_requests: Arc::new(RwLock::new(Vec::new())),
            active_sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Subscribe to receiver events
    pub fn subscribe(&self) -> broadcast::Receiver<ReceiverEvent> {
        self.event_tx.subscribe()
    }

    /// Get the event sender for external use
    pub fn event_sender(&self) -> broadcast::Sender<ReceiverEvent> {
        self.event_tx.clone()
    }

    /// Get receiver configuration
    pub fn config(&self) -> &ReceiverConfig {
        &self.config
    }

    /// Get pending transfer requests
    pub async fn pending_requests(&self) -> Vec<TransferRequest> {
        self.pending_requests.read().await.clone()
    }

    /// Add a pending transfer request
    pub async fn add_pending_request(&self, request: TransferRequest) {
        let mut requests = self.pending_requests.write().await;
        requests.push(request.clone());
        let _ = self.event_tx.send(ReceiverEvent::TransferRequest(request));
    }

    /// Accept a transfer request
    pub async fn accept_transfer(&self, session_id: &str) -> Result<TransferResponse> {
        let mut requests = self.pending_requests.write().await;

        if let Some(pos) = requests.iter().position(|r| r.session_id == session_id) {
            let request = requests.remove(pos);

            let session = TransferSession {
                id: session_id.to_string(),
                request: request.clone(),
                accepted: true,
                files_received: 0,
                total_files: request.files.len(),
            };

            self.active_sessions.write().await.insert(session_id.to_string(), session);

            let _ = self.event_tx.send(ReceiverEvent::TransferAccepted(session_id.to_string()));

            // Get local IP address
            let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

            Ok(TransferResponse {
                accepted: true,
                reason: None,
                ip: Some(local_ip),
                port: Some(self.config.http_port),
            })
        } else {
            Err(DropError::Protocol(format!("Session {} not found", session_id)))
        }
    }

    /// Reject a transfer request
    pub async fn reject_transfer(&self, session_id: &str, reason: Option<String>) -> Result<TransferResponse> {
        let mut requests = self.pending_requests.write().await;

        if let Some(pos) = requests.iter().position(|r| r.session_id == session_id) {
            requests.remove(pos);
            let _ = self.event_tx.send(ReceiverEvent::TransferRejected(session_id.to_string()));

            Ok(TransferResponse {
                accepted: false,
                reason,
                ip: None,
                port: None,
            })
        } else {
            Err(DropError::Protocol(format!("Session {} not found", session_id)))
        }
    }

    /// Get active sessions
    pub async fn active_sessions(&self) -> Vec<TransferSession> {
        self.active_sessions.read().await.values().cloned().collect()
    }

    /// Update session progress
    pub async fn update_session_progress(&self, session_id: &str, file_received: bool) {
        let mut sessions = self.active_sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            if file_received {
                session.files_received += 1;
            }

            if session.files_received >= session.total_files {
                let _ = self.event_tx.send(ReceiverEvent::SessionComplete {
                    session_id: session_id.to_string(),
                });
            }
        }
    }
}

/// Get the local IP address
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;

    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

// Platform-specific device name
mod whoami {
    pub fn devicename() -> String {
        #[cfg(target_os = "windows")]
        {
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".to_string())
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("scutil")
                .args(["--get", "ComputerName"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "Mac".to_string())
        }
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/etc/hostname")
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "Linux PC".to_string())
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            "Drop Device".to_string()
        }
    }
}
