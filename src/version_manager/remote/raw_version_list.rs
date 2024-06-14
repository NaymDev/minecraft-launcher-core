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
  pub async fn fetch() -> Result<RawVersionList, LoadVersionError> {
    Ok(Client::new().get(VERSION_MANIFEST_URL).send().await?.json::<RawVersionList>().await?)
  }
}

#[cfg(test)]
mod tests {
  use reqwest::Client;
  use serde_json::Value;

  use super::*;

  #[tokio::test]
  async fn test_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let json: Value = Client::new().get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").send().await?.json().await?;
    let versions = json["versions"].as_array().unwrap().to_vec();
    for ver in versions {
      let ver_id = ver["id"].as_str().unwrap();
      let ver = MCVersion::from(ver_id.to_string());
      assert_eq!(ver_id.to_string(), ver.to_string());
    }
    Ok(())
  }
}
