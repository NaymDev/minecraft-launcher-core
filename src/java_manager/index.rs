use std::{ collections::HashMap, ops::Deref };

use serde::{ Deserialize, Serialize };

use crate::json::{ manifest::rule::OperatingSystem, Date, Sha1Sum };

use super::jvm_platform_string;

const JRE_INDEX_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct JreIndex(pub HashMap<String, JreIndexEntry>);

impl Deref for JreIndex {
  type Target = HashMap<String, JreIndexEntry>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl JreIndex {
  pub async fn fetch(client: &reqwest::Client) -> reqwest::Result<JreIndex> {
    client.get(JRE_INDEX_URL).send().await?.error_for_status()?.json().await
  }

  pub fn find(&self, os: &OperatingSystem, arch: Option<&str>) -> Option<&JreIndexEntry> {
    if let Some(entry) = self.get(&jvm_platform_string(os, arch)) {
      return Some(entry);
    }

    self.get(&jvm_platform_string(os, None))
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JreIndexEntry(pub HashMap<String, Vec<RuntimeInfo>>);

impl Deref for JreIndexEntry {
  type Target = HashMap<String, Vec<RuntimeInfo>>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeInfo {
  pub availability: RuntimeAvailability,
  pub manifest: RuntimeManifestInfo,
  pub version: RuntimeVersionInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeAvailability {
  group: u32,
  progress: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeManifestInfo {
  pub url: String,
  pub size: u64,
  pub sha1: Sha1Sum,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeVersionInfo {
  pub name: String,
  pub released: Date,
}
