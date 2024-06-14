use std::{ fs::File, path::PathBuf };

use serde::{ Deserialize, Serialize };

use crate::json::{ manifest::VersionManifest, Date, MCVersion, ReleaseType, VersionInfo };

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
  pub fn from_manifest(manifest_path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
    let manifest: VersionManifest = serde_json::from_reader(File::open(manifest_path)?)?;
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
