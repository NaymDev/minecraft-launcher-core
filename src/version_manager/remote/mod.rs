use std::io::Cursor;

use reqwest::Client;
use serde::{ Deserialize, Serialize };

use crate::json::{ manifest::VersionManifest, Date, MCVersion, ReleaseType, Sha1Sum, VersionInfo };

mod raw_version_list;

pub use raw_version_list::RawVersionList;

use super::error::InstallVersionError;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoteVersionInfo {
  id: MCVersion,
  #[serde(rename = "type")]
  release_type: ReleaseType,
  url: String,
  #[serde(rename = "time")]
  updated_time: Date,
  release_time: Date,
  sha1: Sha1Sum,
  compliance_level: u8,
}

impl RemoteVersionInfo {
  pub fn get_url(&self) -> &str {
    &self.url
  }

  pub fn get_sha1(&self) -> &Sha1Sum {
    &self.sha1
  }

  pub fn get_compliance_level(&self) -> u8 {
    self.compliance_level
  }

  pub async fn fetch(&self, client: &Client) -> Result<VersionManifest, InstallVersionError> {
    let bytes = client.get(&self.url).send().await?.error_for_status()?.bytes().await?;
    let sha1 = Sha1Sum::from_reader(&mut Cursor::new(&bytes)).map_err(InstallVersionError::ChecksumError)?;
    if sha1 != self.sha1 {
      return Err(InstallVersionError::ChecksumMismatch { expected: self.sha1.clone(), actual: sha1 });
    }
    Ok(serde_json::from_slice(&bytes[..])?)
  }
}

impl VersionInfo for RemoteVersionInfo {
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
