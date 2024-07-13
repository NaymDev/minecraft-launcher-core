use std::path::PathBuf;

use thiserror::Error;

use crate::{ json::Sha1Sum, version_manager::downloader::{ downloadables::DownloadError, error::Error } };

#[derive(Debug, Error)]
pub enum InstallRuntimeError {
  #[error("Operating system not supported")] UnsupportedOS,
  #[error("Runtime '{component}' not found")] RuntimeNotFound {
    component: String,
  },
  #[error("Failed to install runtime")] InstallFailure,
  #[error("Failed to download manifest: {0}")] ManifestDownload(#[from] reqwest::Error),
  #[error("Failed to parse manifest: {0}")] ManifestParse(#[from] serde_json::Error),
  #[error("Checksum mismatch. Expected {expected}, got {actual}")] ChecksumMismatch {
    expected: Sha1Sum,
    actual: Sha1Sum,
  },

  #[error("Failed to download runtime: {0}")] DownloadFailure(#[from] Error),
  #[error("Failed to create folder '{folder}': {source}")] CreateFolder {
    folder: PathBuf,
    source: std::io::Error,
  },
  #[error("Failed to create symlink '{file}': {source}")] CreateSymlink {
    file: PathBuf,
    source: std::io::Error,
  },
  #[error("Failed to write version file: {0}")] WriteVersionFile(std::io::Error),
}

#[derive(Debug, Error)]
pub enum CompressedRuntimeFileError {
  #[error("Failed to write compressed file: {0}")] WriteLzma(#[source] std::io::Error),
  #[error("Failed to remove compressed file: {0}")] RemoveLzma(#[source] std::io::Error),
  #[error("Failed to read compressed file: {0}")] ReadLzma(#[source] std::io::Error),
  #[error("Couldn't prepare the destination folder for the compressed asset: {0}")] PrepareDestination(#[source] std::io::Error),
  #[error("Failed to download compressed asset: {0}")] Download(#[from] reqwest::Error),
  #[error(
    "Hash did not match downloaded compressed asset (Expected {}, downloaded {})",
    hex::encode(expected),
    hex::encode(actual)
  )] ChecksumMismatch {
    expected: Vec<u8>,
    actual: Vec<u8>,
  },
  #[error("Failed to decompress file: {0}")] Decompress(#[from] lzma_rs::error::Error),
}

impl From<CompressedRuntimeFileError> for DownloadError {
  fn from(val: CompressedRuntimeFileError) -> Self {
    DownloadError::Other(Box::new(val))
  }
}
