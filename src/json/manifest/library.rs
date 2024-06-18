use std::{ collections::HashMap, path::Path };

use reqwest::Url;
use serde::{ Deserialize, Serialize };

use crate::{ download_utils::downloadables::{ ChecksummedDownloadable, Downloadable, PreHashedDownloadable }, json::EnvironmentFeatures };

use super::{ artifact::Artifact, rule::{ OperatingSystem, Rule, RuleAction }, DownloadInfo };

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Library {
  pub name: Artifact,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub rules: Vec<Rule>,
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub natives: HashMap<OperatingSystem, String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub extract: Option<ExtractRules>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub downloads: Option<LibraryDownloadInfo>,
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

  pub fn get_artifact_path(&self, classifier: Option<&str>) -> String {
    let mut new_artifact = self.name.clone();
    if let Some(classifier) = classifier {
      new_artifact.classifier = Some(classifier.to_string());
    }
    new_artifact.get_path_string()
  }

  pub fn create_download(
    &self,
    artifact_path: &str,
    target_file: &Path,
    force_download: bool,
    classifier: Option<&str>
  ) -> Option<Box<dyn Downloadable + Send + Sync>> {
    if let Some(url) = &self.url {
      let mut url = Url::parse(url).ok()?;
      url.set_path(&self.get_artifact_path(classifier));
      Some(Box::new(ChecksummedDownloadable::new(url.as_str(), target_file, force_download)))
    } else if let Some(downloads) = &self.downloads {
      if let Some(info) = downloads.get_download_info(classifier) {
        Some(Box::new(PreHashedDownloadable::new(&info.url, target_file, force_download, info.sha1)))
      } else {
        None
      }
    } else {
      let mut url = Url::parse("https://libraries.minecraft.net/").ok()?;
      url.set_path(artifact_path);
      Some(Box::new(ChecksummedDownloadable::new(url.as_str(), target_file, force_download)))
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
  pub artifact: DownloadInfo,
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub classifiers: HashMap<String, DownloadInfo>,
}

impl LibraryDownloadInfo {
  pub fn get_download_info(&self, classifier: Option<&str>) -> Option<DownloadInfo> {
    if let Some(classifier) = classifier { self.classifiers.get(classifier).cloned() } else { Some(self.artifact.clone()) }
  }
}
