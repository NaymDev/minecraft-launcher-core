use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
  #[error("Failed to write destination file: {0}")] WriteFile(#[source] std::io::Error),
  #[error("Failed to remove invalid file: {0}")] RemoveFile(#[source] std::io::Error),
  #[error("Couldn't prepare destination folder: {0}")] PrepareDestination(#[source] std::io::Error),
  #[error("Failed to download: {0}")] Download(#[from] reqwest::Error),
  #[error("Checksum did not match downloaded file (Checksum was {}, downloaded {})", hex::encode(actual), hex::encode(expected))] ChecksumMismatch {
    expected: Vec<u8>,
    actual: Vec<u8>,
  },
  #[error(transparent)] HashError(#[from] HashError),
  #[error(transparent)] Other(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum HashError {
  #[error("Failed to calculate hash for existing destination file: {0}")] ChecksumFile(#[from] std::io::Error),
  #[error("Failed to fetch expected hash: {0}")] FetchHash(#[from] reqwest::Error),
  #[error("Hash length does not match buffer length (Expected size {expected}, got {actual})")] SizeMismatch {
    expected: usize,
    actual: usize,
  },
  #[error("Failed to parse expected hash: {0}")] ParseHash(#[from] hex::FromHexError),
}
