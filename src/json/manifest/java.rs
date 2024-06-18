use serde::{ Deserialize, Serialize };

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionInfo {
  pub component: String,
  pub major_version: i64,
}

impl Default for JavaVersionInfo {
  fn default() -> Self {
    Self {
      component: "jre-legacy".to_string(),
      major_version: 8,
    }
  }
}
