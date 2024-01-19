use std::{ path::PathBuf, collections::HashMap, sync::{ Mutex, Arc } };

use derive_builder::Builder;
use serde_json::Value;

use crate::{
  versions::{ info::MCVersion, json::rule::{ FeatureMatcher, RuleFeatureType } },
  download_utils::ProxyOptions,
  profile_manager::auth::UserAuthentication,
  monitor::{ Monitor, MockMonitor },
};

#[derive(Debug, Clone, Copy)]
pub struct MinecraftResolution(u32, u32);

impl MinecraftResolution {
  pub fn new(width: u32, height: u32) -> Self {
    Self(width, height)
  }

  pub fn width(&self) -> u32 {
    self.0
  }

  pub fn height(&self) -> u32 {
    self.1
  }
}

#[derive(Debug, Clone)]
pub struct LauncherOptions {
  pub launcher_name: String,
  pub launcher_version: String,
}

impl LauncherOptions {
  pub fn new(launcher_name: &str, launcher_version: &str) -> Self {
    Self { launcher_name: launcher_name.to_string(), launcher_version: launcher_version.to_string() }
  }
}

#[derive(Debug, Builder)]
#[builder(pattern = "owned", setter(strip_option))]
pub struct GameOptions {
  pub version: MCVersion,
  pub game_dir: PathBuf,
  #[builder(default)]
  pub proxy: ProxyOptions,
  #[builder(default)]
  pub resolution: Option<MinecraftResolution>,
  pub java_path: PathBuf,
  pub authentication: Box<dyn UserAuthentication + Send + Sync>,
  #[builder(default)]
  pub launcher_options: Option<LauncherOptions>,
  #[builder(default)]
  pub substitutor_overrides: HashMap<String, String>,
  #[builder(default)]
  pub jvm_args: Option<Vec<String>>,

  #[builder(default = "16")]
  pub max_concurrent_downloads: u16,
  #[builder(default = "5")]
  pub max_download_attempts: u8,

  #[builder(default = "Self::default_download_monitor()")]
  pub download_monitor: Arc<Mutex<dyn Monitor + Send + Sync>>,
}

impl GameOptionsBuilder {
  fn default_download_monitor() -> Arc<Mutex<impl Monitor + Send + Sync>> {
    Arc::new(Mutex::new(MockMonitor::new()))
  }
}

#[derive(Debug, Clone)]
pub struct MinecraftFeatureMatcher(pub bool, pub Option<MinecraftResolution>);

impl MinecraftFeatureMatcher {
  pub fn new(is_demo: bool, custom_resolution: Option<MinecraftResolution>) -> Self {
    Self(is_demo, custom_resolution)
  }
}

impl FeatureMatcher for MinecraftFeatureMatcher {
  fn has_feature(&self, feature_type: &RuleFeatureType, value: &Value) -> bool {
    if let Some(value) = value.as_bool() {
      if let RuleFeatureType::IsDemoUser = feature_type {
        return value == self.0;
      }
      if let RuleFeatureType::HasCustomResolution = feature_type {
        return value == self.1.is_some();
      }
    }
    return false;
  }
}
