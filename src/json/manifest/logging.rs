use serde::{ Deserialize, Serialize };

use crate::json::Sha1Sum;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LoggingEntry {
  pub argument: String, // "-Dlog4j.configurationFile=${path}"
  pub file: LoggingEntryFile,
  #[serde(rename = "type")]
  pub log_type: String, // "log4j2-xml"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LoggingEntryFile {
  pub id: String, // "client-1.12.xml" ("client-1.7.xml" for 1.10.2)
  pub sha1: Sha1Sum,
  pub size: i64,
  pub url: String,
}
