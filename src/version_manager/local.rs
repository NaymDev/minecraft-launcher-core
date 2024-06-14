use std::{ ffi::OsStr, fs::{ canonicalize, File }, path::PathBuf };

use serde::{ Deserialize, Serialize };

use crate::{ json::{ manifest::VersionManifest, Date, MCVersion, ReleaseType, VersionInfo }, MinecraftLauncherError };

use super::error::LoadVersionError;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
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

  pub fn load_manifest(&self) -> Result<VersionManifest, Box<dyn std::error::Error>> {
    Ok(serde_json::from_reader(File::open(&self.manifest_path)?)?)
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
