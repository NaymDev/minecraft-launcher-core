use serde::{ Deserialize, Serialize };

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
