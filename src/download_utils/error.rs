use thiserror::Error;

use crate::json::{ Sha1Sum, Sha1SumError };

#[derive(Error, Debug)]
pub enum Error {
  #[error("failed to download: {0}")] DownloadError(#[from] reqwest::Error),
  #[error(transparent)] IoError(#[from] std::io::Error),
  #[error(transparent)] ChecksumError(#[from] Sha1SumError),
  #[error("Checksum did not match downloaded file (Checksum was {actual}, downloaded {expected})")] ChecksumMismatch {
    expected: Sha1Sum,
    actual: Sha1Sum,
  },
  #[error("failed to prepare destination folder: {0}")] PrepareFolderError(std::io::Error),
  #[error("Couldn't parse URL: {0}")] UrlParseError(String),
  #[error("{0}")] Other(String),
}
