//! File Storage Management
//!
//! Handles saving received files to disk with proper naming,
//! conflict resolution, and directory management.

use crate::{DropError, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use chrono::Local;
use tracing::{info, debug};

/// Manages file storage for received transfers
pub struct StorageManager {
    base_directory: PathBuf,
}

impl StorageManager {
    pub fn new(base_directory: PathBuf) -> Self {
        Self { base_directory }
    }

    /// Get the base save directory
    pub fn base_directory(&self) -> &Path {
        &self.base_directory
    }

    /// Ensure the save directory exists
    pub fn ensure_directory(&self) -> Result<()> {
        if !self.base_directory.exists() {
            fs::create_dir_all(&self.base_directory)
                .map_err(|e| DropError::Storage(format!(
                    "Failed to create directory {}: {}",
                    self.base_directory.display(),
                    e
                )))?;
        }
        Ok(())
    }

    /// Save a file to the storage directory
    ///
    /// Returns the final path where the file was saved.
    /// Automatically handles name conflicts by appending a number.
    pub fn save_file(&self, filename: &str, data: &[u8]) -> Result<PathBuf> {
        self.ensure_directory()?;

        // Sanitize filename
        let safe_filename = sanitize_filename(filename);

        // Get unique path
        let save_path = self.get_unique_path(&safe_filename);

        debug!("Saving file to: {}", save_path.display());

        // Write file
        let mut file = File::create(&save_path)
            .map_err(|e| DropError::Storage(format!(
                "Failed to create file {}: {}",
                save_path.display(),
                e
            )))?;

        file.write_all(data)
            .map_err(|e| DropError::Storage(format!(
                "Failed to write file {}: {}",
                save_path.display(),
                e
            )))?;

        file.flush()
            .map_err(|e| DropError::Storage(format!(
                "Failed to flush file {}: {}",
                save_path.display(),
                e
            )))?;

        info!("Saved {} bytes to {}", data.len(), save_path.display());

        Ok(save_path)
    }

    /// Save a file from a stream/reader
    pub fn save_file_stream<R: std::io::Read>(
        &self,
        filename: &str,
        mut reader: R,
        expected_size: Option<u64>,
    ) -> Result<PathBuf> {
        self.ensure_directory()?;

        let safe_filename = sanitize_filename(filename);
        let save_path = self.get_unique_path(&safe_filename);

        debug!("Saving file stream to: {}", save_path.display());

        let mut file = File::create(&save_path)
            .map_err(|e| DropError::Storage(format!(
                "Failed to create file {}: {}",
                save_path.display(),
                e
            )))?;

        let mut total_written = 0u64;
        let mut buffer = vec![0u8; 65536]; // 64KB buffer

        loop {
            let n = reader.read(&mut buffer)
                .map_err(|e| DropError::Storage(format!("Failed to read from stream: {}", e)))?;

            if n == 0 {
                break;
            }

            file.write_all(&buffer[..n])
                .map_err(|e| DropError::Storage(format!("Failed to write to file: {}", e)))?;

            total_written += n as u64;
        }

        file.flush()
            .map_err(|e| DropError::Storage(format!("Failed to flush file: {}", e)))?;

        if let Some(expected) = expected_size {
            if total_written != expected {
                return Err(DropError::Storage(format!(
                    "Size mismatch: expected {} bytes, got {}",
                    expected,
                    total_written
                )));
            }
        }

        info!("Saved {} bytes to {}", total_written, save_path.display());

        Ok(save_path)
    }

    /// Get a unique file path, appending a number if necessary
    fn get_unique_path(&self, filename: &str) -> PathBuf {
        let base_path = self.base_directory.join(filename);

        if !base_path.exists() {
            return base_path;
        }

        // File exists, need to find a unique name
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);

        let extension = Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{}", s))
            .unwrap_or_default();

        for i in 1..1000 {
            let new_name = format!("{} ({}){}", stem, i, extension);
            let new_path = self.base_directory.join(&new_name);

            if !new_path.exists() {
                return new_path;
            }
        }

        // Fallback: use timestamp
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let new_name = format!("{}_{}{}", stem, timestamp, extension);
        self.base_directory.join(new_name)
    }

    /// Create a subdirectory for a transfer session
    pub fn create_session_directory(&self, session_id: &str) -> Result<PathBuf> {
        let session_dir = self.base_directory.join(format!("drop_{}", &session_id[..8]));

        fs::create_dir_all(&session_dir)
            .map_err(|e| DropError::Storage(format!(
                "Failed to create session directory: {}",
                e
            )))?;

        Ok(session_dir)
    }

    /// List files in the save directory
    pub fn list_files(&self) -> Result<Vec<FileEntry>> {
        self.ensure_directory()?;

        let mut entries = Vec::new();

        for entry in fs::read_dir(&self.base_directory)
            .map_err(|e| DropError::Storage(format!("Failed to read directory: {}", e)))?
        {
            let entry = entry
                .map_err(|e| DropError::Storage(format!("Failed to read entry: {}", e)))?;

            let metadata = entry.metadata()
                .map_err(|e| DropError::Storage(format!("Failed to read metadata: {}", e)))?;

            if metadata.is_file() {
                entries.push(FileEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: entry.path(),
                    size: metadata.len(),
                    modified: metadata.modified().ok(),
                });
            }
        }

        // Sort by modification time, newest first
        entries.sort_by(|a, b| b.modified.cmp(&a.modified));

        Ok(entries)
    }
}

/// A file entry in the storage directory
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

/// Sanitize a filename to remove potentially dangerous characters
fn sanitize_filename(filename: &str) -> String {
    // Characters that are problematic on various filesystems
    const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];

    let mut sanitized: String = filename
        .chars()
        .map(|c| if INVALID_CHARS.contains(&c) { '_' } else { c })
        .collect();

    // Remove leading/trailing spaces and dots (problematic on Windows)
    sanitized = sanitized.trim().trim_matches('.').to_string();

    // Handle reserved names on Windows
    let reserved = ["CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];

    let upper = sanitized.to_uppercase();
    for name in reserved {
        if upper == name || upper.starts_with(&format!("{}.", name)) {
            sanitized = format!("_{}", sanitized);
            break;
        }
    }

    // Ensure non-empty
    if sanitized.is_empty() {
        sanitized = "unnamed_file".to_string();
    }

    // Limit length (255 is common max for most filesystems)
    if sanitized.len() > 200 {
        let stem = &sanitized[..150];
        let ext_start = sanitized.rfind('.').unwrap_or(sanitized.len());
        let ext = &sanitized[ext_start..];

        if ext.len() < 50 {
            sanitized = format!("{}{}", stem, ext);
        } else {
            sanitized = stem.to_string();
        }
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal.txt"), "normal.txt");
        assert_eq!(sanitize_filename("file/with/slashes.txt"), "file_with_slashes.txt");
        assert_eq!(sanitize_filename("file:with:colons.txt"), "file_with_colons.txt");
        assert_eq!(sanitize_filename("...hidden"), "hidden");
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename(""), "unnamed_file");
    }

    #[test]
    fn test_save_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = StorageManager::new(temp_dir.path().to_path_buf());

        let content = b"Hello, World!";
        let path = storage.save_file("test.txt", content).unwrap();

        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), content);
    }

    #[test]
    fn test_unique_path() {
        let temp_dir = TempDir::new().unwrap();
        let storage = StorageManager::new(temp_dir.path().to_path_buf());

        // Save first file
        let path1 = storage.save_file("test.txt", b"first").unwrap();
        assert_eq!(path1.file_name().unwrap(), "test.txt");

        // Save second file with same name
        let path2 = storage.save_file("test.txt", b"second").unwrap();
        assert_eq!(path2.file_name().unwrap(), "test (1).txt");

        // Save third file with same name
        let path3 = storage.save_file("test.txt", b"third").unwrap();
        assert_eq!(path3.file_name().unwrap(), "test (2).txt");
    }
}
