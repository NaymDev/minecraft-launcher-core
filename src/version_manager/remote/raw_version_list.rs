use std::collections::HashMap;

use reqwest::Client;
use serde::{ Deserialize, Serialize };

use crate::{ json::{ MCVersion, ReleaseType }, version_manager::error::LoadVersionError };

use super::RemoteVersionInfo;

const VERSION_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct RawVersionList {
  pub latest: HashMap<ReleaseType, MCVersion>,
  pub versions: Vec<RemoteVersionInfo>,
}

impl RawVersionList {
  /// Fetches the version manifest from Mojang's servers.
  pub async fn fetch(client: &Client) -> Result<RawVersionList, LoadVersionError> {
    Ok(client.get(VERSION_MANIFEST_URL).send().await?.json::<RawVersionList>().await?)
  }
}
