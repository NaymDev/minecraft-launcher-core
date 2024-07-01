use reqwest::Client;
use serde_json::Value;

use crate::{ json::{ MCVersion, VersionInfo }, version_manager::remote::{ RawVersionList, RemoteVersionInfo } };

#[tokio::test]
async fn test_version_id_parsing() -> Result<(), Box<dyn std::error::Error>> {
  let json: Value = Client::new().get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").send().await?.json().await?;
  let versions = json["versions"].as_array().unwrap().to_vec();
  for ver in versions {
    let ver_id = ver["id"].as_str().unwrap();
    let ver = MCVersion::from(ver_id.to_string());
    assert_eq!(ver_id.to_string(), ver.to_string());
  }
  Ok(())
}

#[tokio::test]
async fn test_full_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
  let version_list = RawVersionList::fetch().await?;
  for ver in version_list.versions {
    println!("Processing {}", ver.get_id());
    let ver = ver.fetch().await?;
    println!("{ver:#?}");
  }
  Ok(())
}

#[tokio::test]
async fn test_date_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
  let json: Value = Client::new().get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").send().await?.json().await?;
  let versions = json["versions"].as_array().unwrap().to_vec();
  for ver in &versions {
    let time = ver["time"].as_str().unwrap();
    let release_time = ver["releaseTime"].as_str().unwrap();
    let ver: RemoteVersionInfo = serde_json::from_value(ver.clone())?;
    println!("{ver:?}");
    assert_eq!(serde_json::to_string(ver.get_release_time())?, release_time.to_string());
    assert_eq!(serde_json::to_string(ver.get_updated_time())?, time.to_string());
  }
  Ok(())
}
