use std::{collections::HashMap, env::consts::{OS, ARCH}, fmt::Debug};

use os_info::Version;
use regex::Regex;
use serde::{ Serialize, Deserialize };
use serde_json::Value;

pub trait FeatureMatcher {
  fn has_feature(&self, feature_type: &RuleFeatureType, value: &Value) -> bool;
}

impl Debug for dyn FeatureMatcher + Send + Sync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FeatureMatcher")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
  pub action: RuleAction, // "allow" or "disallow"
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub features: Option<HashMap<RuleFeatureType, Value>>, // Option<RuleFeatures>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub os: Option<OsRestriction>,
}

impl Rule {
  pub fn get_applied_action(&self, feature_matcher: Option<&dyn FeatureMatcher>) -> Option<RuleAction> {
    if self.os.is_some() && !&self.os.as_ref().unwrap().is_current_operating_system() {
      return None;
    } else {
      if let Some(features) = &self.features {
        if let Some(feature_matcher) = feature_matcher {
          for (feature_type, value) in features {
            if !feature_matcher.has_feature(&feature_type, &value) {
              return None;
            }
          }
        } else {
          return None;
        }
      }
      return Some(self.action.clone());
    }
  }
}

//

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
  Allow,
  Disallow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
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

    return true;
  }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Eq, Clone, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    Linux, Windows, Osx, Unknown
}

impl OperatingSystem {
    pub fn values() -> [OperatingSystem; 4] {
        [
            OperatingSystem::Linux,
            OperatingSystem::Windows,
            OperatingSystem::Osx,
            OperatingSystem::Unknown
        ]
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
                    return os
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