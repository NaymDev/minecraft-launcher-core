use thiserror::Error;

use crate::json::Sha1Sum;

#[derive(Error, Debug)]
pub enum Error {
  #[error("failed to download: {0}")] DownloadError(#[from] reqwest::Error),
  #[error(transparent)] IoError(#[from] std::io::Error),
  #[error("Failed to calculate checksum")] ChecksumError(#[source] std::io::Error),
  #[error("Checksum did not match downloaded file (Checksum was {actual}, downloaded {expected})")] ChecksumMismatch {
    expected: Sha1Sum,
    actual: Sha1Sum,
  },
  #[error("failed to prepare destination folder: {0}")] PrepareFolderError(std::io::Error),
  #[error("Couldn't parse URL: {0}")] UrlParseError(String),
  #[error("{0}")] Other(String),

  #[error("Job '{name}' finished with {failures} failure(s)! (took {total_time}s)")] JobFailed {
    name: String,
    failures: usize,
    total_time: i64,
  },
}
