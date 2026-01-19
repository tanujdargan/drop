//! HTTP File Receive Server
//!
//! A lightweight HTTP server for receiving file uploads from Drop senders.
//! This server is started when a transfer is accepted and provides an
//! endpoint for the sender to POST files to.

use crate::{DropError, Result, ReceiverEvent, StorageManager};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error, debug};

/// HTTP server for receiving file uploads
pub struct FileReceiveServer {
    port: u16,
    save_directory: PathBuf,
    event_tx: broadcast::Sender<ReceiverEvent>,
    is_running: Arc<RwLock<bool>>,
    storage: StorageManager,
}

impl FileReceiveServer {
    pub fn new(
        port: u16,
        save_directory: PathBuf,
        event_tx: broadcast::Sender<ReceiverEvent>,
    ) -> Self {
        let storage = StorageManager::new(save_directory.clone());

        Self {
            port,
            save_directory,
            event_tx,
            is_running: Arc::new(RwLock::new(false)),
            storage,
        }
    }

    /// Start the HTTP server
    pub async fn start(&self) -> Result<()> {
        if *self.is_running.read().await {
            return Ok(());
        }

        let addr = format!("0.0.0.0:{}", self.port);
        info!("Starting HTTP file receive server on {}", addr);

        let listener = TcpListener::bind(&addr)
            .map_err(|e| DropError::Http(format!("Failed to bind to {}: {}", addr, e)))?;

        // Set non-blocking for graceful shutdown
        listener.set_nonblocking(true)
            .map_err(|e| DropError::Http(format!("Failed to set non-blocking: {}", e)))?;

        *self.is_running.write().await = true;

        let is_running = self.is_running.clone();
        let save_dir = self.save_directory.clone();
        let event_tx = self.event_tx.clone();

        // Spawn server thread
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();

            loop {
                // Check if we should stop
                if !rt.block_on(async { *is_running.read().await }) {
                    break;
                }

                match listener.accept() {
                    Ok((stream, addr)) => {
                        debug!("Accepted connection from {}", addr);
                        let save_dir = save_dir.clone();
                        let event_tx = event_tx.clone();

                        thread::spawn(move || {
                            if let Err(e) = handle_connection(stream, save_dir, event_tx) {
                                error!("Error handling connection: {}", e);
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No incoming connections, sleep briefly
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(e) => {
                        error!("Error accepting connection: {}", e);
                    }
                }
            }

            info!("HTTP server stopped");
        });

        info!("HTTP server started on port {}", self.port);
        Ok(())
    }

    /// Stop the HTTP server
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping HTTP server");
        *self.is_running.write().await = false;
        Ok(())
    }

    /// Check if server is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Get the server port
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Handle an incoming HTTP connection
fn handle_connection(
    mut stream: TcpStream,
    save_dir: PathBuf,
    event_tx: broadcast::Sender<ReceiverEvent>,
) -> Result<()> {
    let mut buffer = vec![0u8; 8192];
    let mut request_data = Vec::new();

    // Read request headers
    loop {
        let n = stream.read(&mut buffer)
            .map_err(|e| DropError::Http(format!("Failed to read from stream: {}", e)))?;

        if n == 0 {
            break;
        }

        request_data.extend_from_slice(&buffer[..n]);

        // Check if we've received all headers
        if let Some(header_end) = find_header_end(&request_data) {
            let headers = String::from_utf8_lossy(&request_data[..header_end]);
            debug!("Received headers:\n{}", headers);

            // Parse the request
            let (method, path) = parse_request_line(&headers);
            info!("Request: {} {}", method, path);

            match (method, path) {
                ("POST", "/upload") => {
                    return handle_upload(
                        &mut stream,
                        &headers,
                        &request_data[header_end..],
                        save_dir,
                        event_tx,
                    );
                }
                ("GET", "/") | ("GET", "/health") => {
                    send_response(&mut stream, 200, "OK", b"Drop Receiver Ready")?;
                    return Ok(());
                }
                ("OPTIONS", _) => {
                    // Handle CORS preflight
                    send_cors_response(&mut stream)?;
                    return Ok(());
                }
                _ => {
                    send_response(&mut stream, 404, "Not Found", b"Not Found")?;
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

/// Handle file upload
fn handle_upload(
    stream: &mut TcpStream,
    headers: &str,
    initial_body: &[u8],
    save_dir: PathBuf,
    event_tx: broadcast::Sender<ReceiverEvent>,
) -> Result<()> {
    // Parse Content-Length
    let content_length = parse_content_length(headers)
        .ok_or_else(|| DropError::Http("Missing Content-Length header".to_string()))?;

    // Parse Content-Type for boundary
    let boundary = parse_multipart_boundary(headers)
        .ok_or_else(|| DropError::Http("Missing or invalid Content-Type boundary".to_string()))?;

    info!("Receiving upload: {} bytes, boundary: {}", content_length, boundary);

    // Read the full body
    let mut body = initial_body.to_vec();
    let mut buffer = vec![0u8; 65536]; // 64KB buffer

    while body.len() < content_length {
        let n = stream.read(&mut buffer)
            .map_err(|e| DropError::Http(format!("Failed to read body: {}", e)))?;

        if n == 0 {
            break;
        }

        body.extend_from_slice(&buffer[..n]);

        // Report progress periodically
        if body.len() % (1024 * 1024) < 65536 {
            debug!("Received {} / {} bytes", body.len(), content_length);
        }
    }

    // Parse multipart form data
    let (filename, file_data) = parse_multipart_body(&body, &boundary)?;

    info!("Received file: {} ({} bytes)", filename, file_data.len());

    // Save file
    let storage = StorageManager::new(save_dir.clone());
    let save_path = storage.save_file(&filename, &file_data)?;

    info!("Saved file to: {}", save_path.display());

    // Emit event
    let _ = event_tx.send(ReceiverEvent::TransferComplete {
        session_id: "upload".to_string(),
        file_name: filename.clone(),
        save_path: save_path.to_string_lossy().to_string(),
    });

    // Send success response
    let response_body = format!(r#"{{"success":true,"filename":"{}","size":{}}}"#, filename, file_data.len());
    send_json_response(stream, 200, "OK", response_body.as_bytes())?;

    Ok(())
}

/// Parse request line to get method and path
fn parse_request_line(headers: &str) -> (&str, &str) {
    let first_line = headers.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    if parts.len() >= 2 {
        (parts[0], parts[1])
    } else {
        ("", "")
    }
}

/// Find the end of HTTP headers (double CRLF)
fn find_header_end(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

/// Parse Content-Length header
fn parse_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("content-length:") {
            let value = line.split(':').nth(1)?.trim();
            return value.parse().ok();
        }
    }
    None
}

/// Parse multipart boundary from Content-Type header
fn parse_multipart_boundary(headers: &str) -> Option<String> {
    for line in headers.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("content-type:") && lower.contains("multipart/form-data") {
            // Find boundary parameter
            if let Some(boundary_start) = line.find("boundary=") {
                let boundary = &line[boundary_start + 9..];
                // Remove quotes if present
                let boundary = boundary.trim_matches('"').trim();
                return Some(boundary.to_string());
            }
        }
    }
    None
}

/// Parse multipart body to extract filename and data
fn parse_multipart_body(body: &[u8], boundary: &str) -> Result<(String, Vec<u8>)> {
    let boundary_marker = format!("--{}", boundary);
    let body_str = String::from_utf8_lossy(body);

    // Find the file part
    let parts: Vec<&str> = body_str.split(&boundary_marker).collect();

    for part in parts {
        if part.contains("Content-Disposition") && part.contains("filename=") {
            // Extract filename
            let filename = extract_filename(part)
                .ok_or_else(|| DropError::Http("Could not extract filename".to_string()))?;

            // Find content start (after headers)
            if let Some(content_start) = part.find("\r\n\r\n") {
                let content = &part[content_start + 4..];

                // Remove trailing boundary/CRLF
                let content = content.trim_end_matches("\r\n").trim_end_matches("--").trim_end_matches("\r\n");

                return Ok((filename, content.as_bytes().to_vec()));
            }
        }
    }

    Err(DropError::Http("Could not parse multipart body".to_string()))
}

/// Extract filename from Content-Disposition header
fn extract_filename(part: &str) -> Option<String> {
    for line in part.lines() {
        if line.contains("filename=") {
            // Find filename="..." or filename=...
            if let Some(start) = line.find("filename=") {
                let rest = &line[start + 9..];
                let filename = if rest.starts_with('"') {
                    // Quoted filename
                    rest[1..].split('"').next()?
                } else {
                    // Unquoted filename
                    rest.split(&[';', ' ', '\r', '\n'][..]).next()?
                };
                return Some(filename.to_string());
            }
        }
    }
    None
}

/// Send HTTP response
fn send_response(stream: &mut TcpStream, status: u16, status_text: &str, body: &[u8]) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: POST, GET, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\
         \r\n",
        status, status_text, body.len()
    );

    stream.write_all(response.as_bytes())
        .map_err(|e| DropError::Http(format!("Failed to write response: {}", e)))?;
    stream.write_all(body)
        .map_err(|e| DropError::Http(format!("Failed to write body: {}", e)))?;
    stream.flush()
        .map_err(|e| DropError::Http(format!("Failed to flush: {}", e)))?;

    Ok(())
}

/// Send JSON response
fn send_json_response(stream: &mut TcpStream, status: u16, status_text: &str, body: &[u8]) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: POST, GET, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\
         \r\n",
        status, status_text, body.len()
    );

    stream.write_all(response.as_bytes())
        .map_err(|e| DropError::Http(format!("Failed to write response: {}", e)))?;
    stream.write_all(body)
        .map_err(|e| DropError::Http(format!("Failed to write body: {}", e)))?;
    stream.flush()
        .map_err(|e| DropError::Http(format!("Failed to flush: {}", e)))?;

    Ok(())
}

/// Send CORS preflight response
fn send_cors_response(stream: &mut TcpStream) -> Result<()> {
    let response = "HTTP/1.1 204 No Content\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: POST, GET, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Access-Control-Max-Age: 86400\r\n\
         Connection: close\r\n\
         \r\n";

    stream.write_all(response.as_bytes())
        .map_err(|e| DropError::Http(format!("Failed to write CORS response: {}", e)))?;
    stream.flush()
        .map_err(|e| DropError::Http(format!("Failed to flush: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request_line() {
        let headers = "POST /upload HTTP/1.1\r\nHost: localhost\r\n";
        let (method, path) = parse_request_line(headers);
        assert_eq!(method, "POST");
        assert_eq!(path, "/upload");
    }

    #[test]
    fn test_parse_content_length() {
        let headers = "POST /upload HTTP/1.1\r\nContent-Length: 12345\r\n";
        assert_eq!(parse_content_length(headers), Some(12345));
    }

    #[test]
    fn test_parse_multipart_boundary() {
        let headers = "Content-Type: multipart/form-data; boundary=----WebKitFormBoundary123\r\n";
        assert_eq!(
            parse_multipart_boundary(headers),
            Some("----WebKitFormBoundary123".to_string())
        );
    }
}
