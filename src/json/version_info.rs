use super::{ Date, MCVersion, ReleaseType };

pub trait VersionInfo {
  fn get_id(&self) -> &MCVersion;
  fn get_type(&self) -> &ReleaseType;
  fn get_updated_time(&self) -> &Date;
  fn get_release_time(&self) -> &Date;
}
