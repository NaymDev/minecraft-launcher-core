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
      component: format!("jre-legacy"),
      major_version: 8,
    }
  }
}
