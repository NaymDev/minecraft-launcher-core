use thiserror::Error;

use crate::json::{ Sha1Sum, Sha1SumError };

#[derive(Error, Debug)]
pub enum LoadVersionError {
  #[error("failed to fetch remote version list")] FetchError(#[from] reqwest::Error),
  #[error("not a directory")]
  NotADirectory,
  #[error("invalid version directory")]
  InvalidVersionDir,
  #[error("manifest not found")]
  ManifestNotFound,
  #[error("failed to parse manifest: {0}")] ManifestParseError(#[from] serde_json::Error),
  #[error(transparent)] IoError(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum InstallVersionError {
  #[error("failed to fetch")] FetchError(#[from] reqwest::Error),
  #[error("checksum mismatch, expected {expected}, got {actual}")] ChecksumMismatch {
    expected: Sha1Sum,
    actual: Sha1Sum,
  },
  #[error("failed to parse: {0}")] ParseError(#[from] serde_json::Error),
  #[error(transparent)] ChecksumError(#[from] Sha1SumError),
  #[error(transparent)] IoError(#[from] std::io::Error),
}
