use thiserror::Error;

use crate::json::{ MCVersion, Sha1Sum };

#[derive(Error, Debug)]
pub enum LoadVersionError {
  #[error("version not found: {0}")] VersionNotFound(String),
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
  #[error("version not found: {0}")] VersionNotFound(String),
  #[error("failed to fetch")] FetchError(#[from] reqwest::Error),
  #[error("checksum mismatch, expected {expected}, got {actual}")] ChecksumMismatch {
    expected: Sha1Sum,
    actual: Sha1Sum,
  },
  #[error("failed to parse: {0}")] ParseError(#[from] serde_json::Error),
  #[error("failed to read checksum: {0}")] ChecksumError(#[source] std::io::Error),
  #[error(transparent)] IoError(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ResolveManifestError {
  #[error("Circular dependency detected! {} -> [{}]", inheritance_trace.join(" -> "), problem)] CircularDependency {
    inheritance_trace: Vec<String>,
    problem: MCVersion,
  },
  #[error(transparent)] InstallVersionError(#[from] InstallVersionError),
}
