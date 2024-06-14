use thiserror::Error;

#[derive(Error, Debug)]
pub enum LoadVersionError {
  #[error("not a directory")]
  NotADirectory,
  #[error("invalid version directory")]
  InvalidVersionDir,
  #[error("manifest not found")]
  ManifestNotFound,
  #[error("failed to parse manifest: {0}")] ManifestParseError(#[from] serde_json::Error),
  #[error(transparent)] IoError(#[from] std::io::Error),
}
