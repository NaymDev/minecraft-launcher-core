use serde::{ Deserialize, Serialize };

use crate::json::Sha1Sum;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DownloadType {
  Client,
  Server,
  WindowsServer,
  ClientMappings,
  ServerMappings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
  pub sha1: Sha1Sum,
  pub size: i64,
  pub url: String,
}
