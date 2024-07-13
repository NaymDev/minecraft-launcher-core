use std::{ collections::HashMap, path::PathBuf };

use serde::{ Deserialize, Serialize };

use crate::json::Sha1Sum;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JreManifest {
  pub files: HashMap<PathBuf, JavaRuntimeFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum JavaRuntimeFile {
  #[serde(rename = "file")] File {
    downloads: Downloads,
    #[serde(default)]
    executable: bool,
  },
  #[serde(rename = "directory")]
  Directory,
  #[serde(rename = "link")] Link {
    target: String,
  },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Downloads {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub lzma: Option<Download>,
  pub raw: Download,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Download {
  pub sha1: Sha1Sum,
  pub size: u64,
  pub url: String,
}
