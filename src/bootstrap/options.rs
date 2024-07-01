use std::{ path::PathBuf, collections::HashMap, fmt::Debug };

use derive_builder::Builder;
use serde_json::json;

use crate::json::{ manifest::rule::RuleFeatureType, EnvironmentFeatures };
use super::auth::UserAuthentication;

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

#[derive(Debug, Clone, Default)]
pub enum ProxyOptions {
  Proxy {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
  },
  #[default] NoProxy,
}

impl ProxyOptions {
  pub fn create_http_proxy(&self) -> Option<reqwest::Proxy> {
    if let ProxyOptions::Proxy { host, port, username, password } = self {
      let mut proxy = reqwest::Proxy::all(format!("{}:{}", host, port)).ok()?;
      if let (Some(username), Some(password)) = (username, password) {
        proxy = proxy.basic_auth(username, password);
      }
      Some(proxy)
    } else {
      None
    }
  }
}

#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned", setter(strip_option))]
pub struct GameOptions {
  /// Path to the Java executable
  pub java_path: PathBuf,
  /// Path to the game directory
  pub game_dir: PathBuf,
  /// Path to the natives directory
  pub natives_dir: PathBuf,

  /// The user authentication
  pub authentication: UserAuthentication,

  #[builder(default)]
  pub resolution: Option<(u32, u32)>,
  #[builder(default)]
  pub demo: Option<bool>,

  #[builder(default)]
  pub proxy: ProxyOptions,

  #[builder(default)]
  pub launcher_options: Option<LauncherOptions>,

  #[builder(default)]
  /// Overrides the default arguments applied to the JVM
  pub jvm_args: Option<Vec<String>>,
  #[builder(default)]
  /// Custom substitution rules applied to the JVM and game argument variables
  pub substitutor_overrides: HashMap<String, String>,

  #[builder(default)]
  /// The name of the version displayed inside the game (id of the version by default)
  pub version_name: Option<String>,
}

impl GameOptions {
  pub fn env_features(&self) -> EnvironmentFeatures {
    let mut env_features = EnvironmentFeatures::new();
    if let Some(demo) = self.demo {
      env_features.set_feature(RuleFeatureType::IsDemoUser, json!(demo));
    }
    if self.resolution.is_some() {
      env_features.set_feature(RuleFeatureType::HasCustomResolution, json!(true));
    }
    env_features
  }
}
