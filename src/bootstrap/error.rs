use std::{ path::PathBuf, time::SystemTimeError };

use thiserror::Error;
use zip::result::ZipError;

use crate::version_manager::error::{ LoadVersionError, ResolveManifestError };

#[derive(Debug, Error)]
pub enum Error {
  #[error(transparent)] IO(#[from] std::io::Error),
  #[error("Couldn't load version! {0}")] LoadVersion(#[from] LoadVersionError),
  #[error("Couldn't resolve version! {0}")] ResolveManifest(#[from] ResolveManifestError),
  #[error("Couldn't unpack natives! {0}")] UnpackNatives(UnpackNativesError),
  #[error("Couldn't unpack assets! {0}")] UnpackAssets(UnpackAssetsError),
  #[error("Aborting launch; {0}")] Launch(&'static str),
  #[error("Failed to launch game")] Game(Box<dyn std::error::Error>),
  #[error(transparent)] Pattern(#[from] regex::Error),
  #[error(transparent)] SystemTime(#[from] SystemTimeError),
  #[error(transparent)] Zip(#[from] zip::result::ZipError),
  #[error("Classpath file not found: {0}")] ClasspathFileNotFound(PathBuf),
  #[error("Invalid classpath path: {0}")] InvalidClasspathPath(PathBuf),
}

#[derive(Debug, Error)]
pub enum UnpackNativesError {
  #[error("Failed to create natives folder: {0}")] CreateNativesFolder(std::io::Error),
  #[error("Failed to read native: {0}")] ReadNative(std::io::Error),
  #[error("Failed to unzip native: {0}")] UnzipNative(#[from] ZipError),
  #[error("Failed to unpack native: {0}")] UnpackNative(std::io::Error),
}

#[derive(Debug, Error)]
pub enum UnpackAssetsError {
  #[error("No asset index found in version manifest")] NoAssetIndex,
  #[error("Failed to parse asset index: {0}")] ParseAssetIndex(Box<dyn std::error::Error>),
  #[error("Failed to read asset object: {0}")] ReadAssetObject(std::io::Error),
  #[error("Failed to calculate checksum for asset object: {0}")] ChecksumAssetObject(#[source] std::io::Error),
  #[error("Failed to unpack asset object: {0}")] UnpackAssetObject(std::io::Error),
}
