use std::{ fmt::Debug, io::Cursor };

use regex::Regex;
use serde::{ Deserialize, Serialize };

use crate::MinecraftLauncherError;

use super::json::{ LocalVersionInfo, Sha1Sum, date::Date };

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(untagged, from = "String", into = "String")]
pub enum MCVersion {
  Release(i32, i32, Option<i32>), // 1.20.4 (major, minor, patch)
  Snapshot(i32, i32, String), // 23w46a (two-digit-year, two-digit-week, revision)
  PreReleaseNew(i32, i32, Option<i32>, i32), // 1.14 Pre-Release 4 (major, minor, patch, prerelease_version)
  PreReleaseOld(i32, i32, Option<i32>, i32), // 1.9.1-pre2 (major, minor, patch, prerelease_version)
  ReleaseCandidate(i32, i32, Option<i32>, i32), // 1.19.3-rc3 (major, minor, patch, rc_version)
  Other(String), // Old betas/alphas
}

impl MCVersion {
  pub fn new(value: impl AsRef<str>) -> MCVersion {
    MCVersion::from(value.as_ref().to_string())
  }
}

impl From<String> for MCVersion {
  fn from(value: String) -> Self {
    let release_re = Regex::new(r"^(?P<major>\d+)\.(?P<minor>\d+)(?:\.(?P<patch>\d+))?$").unwrap();
    let snapshot_re = Regex::new(r"^(?P<year>\d{2})w(?P<week>\d{2})(?P<revision>.)$").unwrap();
    let pre_release_new_re = Regex::new(r"^(?P<major>\d+)\.(?P<minor>\d+)(?:\.(?P<patch>\d+))? Pre-Release (?P<prerelease>\d+)$").unwrap();
    let pre_release_old_re = Regex::new(r"^(?P<major>\d+)\.(?P<minor>\d+)(?:\.(?P<patch>\d+))?-pre(?P<prerelease>\d+)$").unwrap();
    let release_candidate_re = Regex::new(r"^(?P<major>\d+)\.(?P<minor>\d+)(?:\.(?P<patch>\d+))?-rc(?P<rc>\d+)$").unwrap();

    // Release -> "{major}.{minor}.{patch}"
    if let Some(caps) = release_re.captures(&value) {
      let major: i32 = caps
        .name("major")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let minor: i32 = caps
        .name("minor")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let patch: Option<i32> = caps.name("patch").map(|m| m.as_str().parse().unwrap());
      return Self::Release(major, minor, patch);
    }
    // Snapshot -> "{two-digit-year}w{two-digit-week}{revision}"
    if let Some(caps) = snapshot_re.captures(&value) {
      let year: i32 = caps
        .name("year")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let week: i32 = caps
        .name("week")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let revision: String = caps
        .name("revision")
        .map(|m| m.as_str())
        .unwrap()
        .to_string();
      return Self::Snapshot(year, week, revision);
    }
    // Pre-release (new) -> "{major}.{minor}.{patch} Pre-Release {prerelease}"
    if let Some(caps) = pre_release_new_re.captures(&value) {
      let major: i32 = caps
        .name("major")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let minor: i32 = caps
        .name("minor")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let patch: Option<i32> = caps.name("patch").map(|m| m.as_str().parse().unwrap());
      let prerelease: i32 = caps
        .name("prerelease")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      return Self::PreReleaseNew(major, minor, patch, prerelease);
    }
    // Pre-release (old) -> "{major}.{minor}.{patch}-pre{prerelease}"
    if let Some(caps) = pre_release_old_re.captures(&value) {
      let major: i32 = caps
        .name("major")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let minor: i32 = caps
        .name("minor")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let patch: Option<i32> = caps.name("patch").map(|m| m.as_str().parse().unwrap());
      let prerelease: i32 = caps
        .name("prerelease")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      return Self::PreReleaseOld(major, minor, patch, prerelease);
    }
    // Release candidate -> "{major}.{minor}.{patch}-rc{rc}"
    if let Some(caps) = release_candidate_re.captures(&value) {
      let major: i32 = caps
        .name("major")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let minor: i32 = caps
        .name("minor")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      let patch: Option<i32> = caps.name("patch").map(|m| m.as_str().parse().unwrap());
      let rc: i32 = caps
        .name("rc")
        .map(|m| m.as_str())
        .unwrap()
        .parse()
        .unwrap();
      return Self::ReleaseCandidate(major, minor, patch, rc);
    }
    return Self::Other(value);
  }
}

impl ToString for MCVersion {
  fn to_string(&self) -> String {
    match &self {
      Self::Release(major, minor, patch) => {
        let mut s = format!("{major}.{minor}");
        if let Some(patch) = patch {
          s.push_str(&format!(".{patch}"));
        }
        s
      }
      Self::Snapshot(year, week, revision) => { format!("{year:02}w{week:02}{revision}") }
      Self::PreReleaseNew(major, minor, patch, prerelease) => {
        let mut s = format!("{major}.{minor}");
        if let Some(patch) = patch {
          s.push_str(&format!(".{patch}"));
        }
        s.push_str(&format!(" Pre-Release {prerelease}"));
        s
      }
      Self::PreReleaseOld(major, minor, patch, prerelease) => {
        let mut s = format!("{major}.{minor}");
        if let Some(patch) = patch {
          s.push_str(&format!(".{patch}"));
        }
        s.push_str(&format!("-pre{prerelease}"));
        s
      }
      Self::ReleaseCandidate(major, minor, patch, rc) => {
        let mut s = format!("{major}.{minor}");
        if let Some(patch) = patch {
          s.push_str(&format!(".{patch}"));
        }
        s.push_str(&format!("-rc{rc}"));
        s
      }
      Self::Other(value) => value.clone(),
    }
  }
}

impl Into<String> for MCVersion {
  fn into(self) -> String {
    self.to_string()
  }
}

impl Debug for MCVersion {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.to_string())
  }
}

//

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
  Release,
  Snapshot,
  OldBeta,
  OldAlpha,
}

impl ReleaseType {
  pub fn get_name(&self) -> &str {
    match self {
      Self::Release => "release",
      Self::Snapshot => "snapshot",
      Self::OldBeta => "old_beta",
      Self::OldAlpha => "old_alpha",
    }
  }
}

//

pub trait VersionInfo {
  fn get_id(&self) -> &MCVersion;
  fn get_type(&self) -> &ReleaseType;
  fn get_updated_time(&self) -> &Date;
  fn get_release_time(&self) -> &Date;
}

//

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

  pub async fn fetch(&self) -> Result<LocalVersionInfo, Box<dyn std::error::Error>> {
    let bytes = reqwest::get(&self.url).await?.bytes().await?;
    let sha1 = Sha1Sum::from_reader(&mut Cursor::new(&bytes))?;
    if sha1 != self.sha1 {
      Err(MinecraftLauncherError(format!("Sha1 mismatch: {sha1} != {}", self.sha1)))?;
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

//

#[cfg(test)]
mod tests {
  use reqwest::Client;
  use serde_json::Value;

  use crate::versions::json::RawVersionList;

  use super::*;

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
      println!("Processing {}", ver.id.to_string());
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
}
