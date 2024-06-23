use std::{ path::PathBuf, time::SystemTimeError };

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
  #[error(transparent)] IO(#[from] std::io::Error),
  #[error("Couldn't unpack natives! {0}")] UnpackNatives(Box<dyn std::error::Error>),
  #[error("Aborting launch; {0}")] Launch(&'static str),
  #[error("Failed to launch game")] Game(Box<dyn std::error::Error>),
  #[error(transparent)] Pattern(#[from] regex::Error),
  #[error(transparent)] SystemTime(#[from] SystemTimeError),
  #[error(transparent)] Zip(#[from] zip::result::ZipError),
  #[error("Classpath file not found: {0}")] ClasspathFileNotFound(PathBuf),
}
