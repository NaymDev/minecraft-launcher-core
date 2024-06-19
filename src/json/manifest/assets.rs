use std::collections::HashMap;

use serde::{ Deserialize, Serialize };

use crate::json::Sha1Sum;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexInfo {
  pub id: String,
  pub sha1: Sha1Sum,
  pub size: i64,
  pub total_size: i64,
  pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
  pub objects: HashMap<String, AssetObject>,
  #[serde(default)]
  pub map_to_resources: bool,
  #[serde(default, rename = "virtual")]
  pub is_virtual: bool,
}

impl AssetIndex {
  pub fn get_file_map(&self) -> HashMap<&String, &AssetObject> {
    self.objects.iter().collect()
  }

  pub fn get_unique_objects(&self) -> HashMap<&AssetObject, &String> {
    self.objects
      .iter()
      .map(|(k, v)| (v, k))
      .collect()
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct AssetObject {
  pub hash: Sha1Sum,
  pub size: usize,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reconstruct: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub compressed_hash: Option<Sha1Sum>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub compressed_size: Option<u64>,
}

impl AssetObject {
  pub fn has_compressed_alternative(&self) -> bool {
    self.compressed_hash.is_some() && self.compressed_size.is_some()
  }

  pub fn create_path_from_hash(hash: &Sha1Sum) -> String {
    let hash = hash.to_string();
    format!("{}/{}", &hash[0..2], hash)
  }
}
