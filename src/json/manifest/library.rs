use std::{ collections::HashMap, env::consts::ARCH, path::Path };

use serde::{ Deserialize, Serialize };

use crate::json::EnvironmentFeatures;

use super::{ artifact::Artifact, rule::{ OperatingSystem, Rule, RuleAction }, DownloadInfo };

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Library {
  pub name: Artifact,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub rules: Vec<Rule>,
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub natives: HashMap<OperatingSystem, String>, // OS -> Artifact Classifier
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub extract: Option<ExtractRules>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub url: Option<String>, // Single download URL
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub downloads: Option<LibraryDownloadInfo>, // Multi download URL (common artifact, or classified artifacts)
}

impl Library {
  pub fn applies_to_current_environment(&self, env_features: &EnvironmentFeatures) -> bool {
    if self.rules.is_empty() {
      return true;
    }

    let mut action = RuleAction::Disallow;
    for rule in &self.rules {
      if let Some(applied_action) = rule.get_applied_action(env_features) {
        action = applied_action;
      }
    }

    action == RuleAction::Allow
  }

  pub fn get_artifact_path(&self, classifier: Option<String>) -> String {
    let mut new_artifact = self.name.clone();
    if let Some(classifier) = classifier {
      new_artifact.classifier = Some(classifier);
    }
    new_artifact.get_path_string()
  }

  pub fn get_artifact_classifier(&self, os: &OperatingSystem) -> Option<Option<String>> {
    if self.natives.is_empty() {
      return Some(None);
    }

    if let Some(classifier) = self.natives.get(os) {
      let arch = if ARCH == "x86" { "32" } else { "64" };
      let classifier = classifier.replace("${arch}", arch);
      return Some(Some(classifier));
    }

    None
  }

  pub fn get_download_info(&self, os: &OperatingSystem) -> Option<DownloadInfo> {
    let classifier = self.get_artifact_classifier(os)?;

    if let Some(downloads) = &self.downloads {
      downloads.get_download_info(classifier)
    } else {
      None
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtractRules {
  pub exclude: Vec<String>,
}

impl ExtractRules {
  pub fn should_extract(&self, zip_path: &Path) -> bool {
    for entry in &self.exclude {
      if zip_path.starts_with(entry) {
        return false;
      }
    }
    true
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryDownloadInfo {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub artifact: Option<DownloadInfo>,
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub classifiers: HashMap<String, DownloadInfo>,
}

impl LibraryDownloadInfo {
  pub fn get_download_info(&self, classifier: Option<String>) -> Option<DownloadInfo> {
    if let Some(classifier) = classifier { self.classifiers.get(&classifier).cloned() } else { self.artifact.clone() }
  }
}
