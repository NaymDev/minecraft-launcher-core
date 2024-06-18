use std::{ collections::HashMap, env::consts::{ OS, ARCH }, fmt::Debug };

use os_info::Version;
use regex::Regex;
use serde::{ Serialize, Deserialize };
use serde_json::Value;

use crate::json::EnvironmentFeatures;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
  /// The action to take if the rule is matched
  pub action: RuleAction,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  /// The features to match in order to take the action
  pub features: Option<HashMap<RuleFeatureType, Value>>, // Option<RuleFeatures>,
  /// The operating system restrictions for the rule
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub os: Option<OsRestriction>,
}

impl Rule {
  /// Retrieves the action to be applied based on the current environment features.
  ///
  /// This function checks if the current environment meets the necessary conditions
  /// defined in `self`. It considers operating system restrictions and other feature
  /// restrictions specified in `self`.
  ///
  /// # Arguments
  ///
  /// * `env_features` - A struct containing the features of the current environment.
  ///
  /// # Returns
  ///
  /// Returns `Some(RuleAction)` if the current environment meets all the conditions;
  /// otherwise, it returns `None` if any condition is not met.

  pub fn get_applied_action(&self, env_features: &EnvironmentFeatures) -> Option<RuleAction> {
    // If there's some operating system restriction, check if it's met
    if let Some(os) = &self.os {
      if !os.is_current_operating_system() {
        return None;
      }
    }

    // If there's some feature restriction, check if it's met
    if !env_features.compatible(self) {
      return None;
    }

    // Everything's fine, return the action
    Some(self.action)
  }
}

//

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
  Allow,
  Disallow,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RuleFeatureType {
  IsDemoUser,
  HasCustomResolution,
  HasQuickPlaysSupport,
  IsQuickPlaySingleplayer,
  IsQuickPlayMultiplayer,
  IsQuickPlayRealms,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OsRestriction {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<OperatingSystem>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub arch: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub version: Option<String>, // Regex
}

impl OsRestriction {
  pub fn is_current_operating_system(&self) -> bool {
    let OsRestriction { name, arch, version } = &self;

    if let Some(name) = name {
      if &OperatingSystem::get_current_platform() != name {
        return false;
      }
    }

    if let Some(arch) = arch {
      if &get_arch() != arch {
        return false;
      }
    }

    if let Some(version) = version {
      if let Ok(regex) = Regex::new(version) {
        if !regex.is_match(&get_os_version()) {
          return false;
        }
      }
    }

    true
  }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Eq, Clone, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
  Linux,
  Windows,
  Osx,
  Unknown,
}

impl OperatingSystem {
  pub fn values() -> [OperatingSystem; 4] {
    [OperatingSystem::Linux, OperatingSystem::Windows, OperatingSystem::Osx, OperatingSystem::Unknown]
  }

  pub fn get_name(&self) -> String {
    let name = match self {
      OperatingSystem::Linux => "linux",
      OperatingSystem::Windows => "windows",
      OperatingSystem::Osx => "osx",
      OperatingSystem::Unknown => "unknown",
    };
    name.to_string()
  }

  pub fn get_aliases(&self) -> Vec<&str> {
    match self {
      OperatingSystem::Linux => vec!["linux", "unix"],
      OperatingSystem::Windows => vec!["win"],
      OperatingSystem::Osx => vec!["mac"],
      OperatingSystem::Unknown => vec![],
    }
  }

  pub fn is_supported(&self) -> bool {
    self != &Self::Unknown
  }

  pub fn get_current_platform() -> Self {
    let os_name = OS.to_lowercase();
    let values = Self::values();
    for os in values {
      let aliases = os.get_aliases();
      for alias in aliases {
        if os_name.contains(alias) {
          return os;
        }
      }
    }
    Self::Unknown
  }
}

pub fn get_arch() -> String {
  let arch = match ARCH {
    "x86_64" => "x64",
    "x86" => "x86",
    s => s,
  };
  arch.to_string()
}

pub fn get_os_version() -> String {
  match os_info::get().version() {
    Version::Semantic(major, minor, patch) => format!("{}.{}.{}", major, minor, patch),
    Version::Custom(version) => version.clone(),
    _ => "unknown".to_string(),
  }
}
