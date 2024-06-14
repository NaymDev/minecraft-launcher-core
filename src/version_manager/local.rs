use std::{ ffi::OsStr, fs::{ canonicalize, File }, path::PathBuf };

use serde::{ Deserialize, Serialize };

use crate::json::{ manifest::VersionManifest, Date, MCVersion, ReleaseType, VersionInfo };

use super::error::LoadVersionError;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
/// A simplified representation of a ManifestVersion.
pub struct LocalVersionInfo {
  id: MCVersion,
  #[serde(rename = "type")]
  release_type: ReleaseType,
  #[serde(rename = "time")]
  updated_time: Date,
  release_time: Date,

  manifest_path: PathBuf,
}

impl LocalVersionInfo {
  /// Creates a new instance of the object.
  ///
  /// # Arguments
  /// * `version_manifest` - A reference to a `VersionManifest` containing metadata about the version.
  /// * `manifest_path` - A reference to a `PathBuf` that specifies the path to the manifest file.
  ///
  /// # Returns
  /// Returns a new instance of the object initialized with values from `version_manifest` and `manifest_path`.
  pub fn new(version_manifest: &VersionManifest, manifest_path: &PathBuf) -> Self {
    Self {
      id: version_manifest.get_id().clone(),
      release_type: version_manifest.get_type().clone(),
      updated_time: version_manifest.get_updated_time().clone(),
      release_time: version_manifest.get_release_time().clone(),
      manifest_path: manifest_path.clone(),
    }
  }

  /// Load a version from the given path.
  /// It's meant to be used with the versions/{version_id} directory.
  /// # Errors
  /// Will return an error if the directory version is not valid.
  pub fn load(version_dir: &PathBuf) -> Result<Self, LoadVersionError> {
    let version_dir = canonicalize(version_dir).map_err(|_| LoadVersionError::InvalidVersionDir)?;
    if !version_dir.is_dir() {
      return Err(LoadVersionError::NotADirectory);
    }
    let version_id = version_dir.file_name().and_then(OsStr::to_str).ok_or(LoadVersionError::InvalidVersionDir)?;
    let manifest_path = version_dir.join(format!("{}.json", version_id));
    if !manifest_path.is_file() {
      return Err(LoadVersionError::ManifestNotFound);
    }
    Self::from_manifest(&manifest_path)
  }

  /// Load a version from the given manifest path.
  /// # Errors
  /// Will return an error if the manifest file is not found.
  pub fn from_manifest(manifest_path: &PathBuf) -> Result<Self, LoadVersionError> {
    let file = File::open(manifest_path)?;
    let manifest: VersionManifest = serde_json::from_reader(file)?;
    Ok(Self {
      id: manifest.get_id().clone(),
      release_type: manifest.get_type().clone(),
      updated_time: manifest.get_updated_time().clone(),
      release_time: manifest.get_release_time().clone(),
      manifest_path: manifest_path.clone(),
    })
  }

  pub fn get_manifest_path(&self) -> &PathBuf {
    &self.manifest_path
  }

  /// Get the full manifest of the version.
  pub fn load_manifest(&self) -> Result<VersionManifest, LoadVersionError> {
    if !self.manifest_path.is_file() {
      return Err(LoadVersionError::ManifestNotFound);
    }
    let file = File::open(&self.manifest_path)?;
    Ok(serde_json::from_reader(file)?)
  }
}

impl VersionInfo for LocalVersionInfo {
  fn get_id(&self) -> &MCVersion {
    &self.id
  }

  fn get_type(&self) -> &ReleaseType {
    &self.release_type
  }

  fn get_updated_time(&self) -> &Date {
    &self.updated_time
  }

  fn get_release_time(&self) -> &Date {
    &self.release_time
  }
}
