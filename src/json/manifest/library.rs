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

  pub fn get_artifact_path(&self, classifier: Option<&str>) -> String {
    let mut new_artifact = self.name.clone();
    if let Some(classifier) = classifier {
      new_artifact.classifier = Some(classifier.to_string());
    }
    new_artifact.get_path_string()
  }

  pub fn get_artifact_classifier(&self, os: &OperatingSystem) -> Option<Option<&str>> {
    if self.natives.is_empty() {
      return Some(None);
    }

    if let Some(classifier) = self.natives.get(os) {
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

  pub fn create_download(&self, game_dir: &Path, os: &OperatingSystem, force_download: bool) -> Option<Box<dyn Downloadable + Send + Sync>> {
    // If the lib has a natives field, but the os is not supported, return None immediately
    let classifier = self.get_artifact_classifier(os)?;

    let libraries_dir = game_dir.join("libraries");
    let file_path = self.name.get_local_path(&libraries_dir);
    let artifact_path = self.get_artifact_path(classifier);

    // If the lib has a single url
    if let Some(url) = &self.url {
      let mut url = Url::parse(url).ok()?;
      url.set_path(&artifact_path);
      let downloadable = ChecksummedDownloadable::new(url.as_str(), &file_path, force_download);
      return Some(Box::new(downloadable));
    }

    // If the lib has no url, try the default download server
    if self.downloads.is_none() {
      let url = format!("https://libraries.minecraft.net/{}", &artifact_path);
      return Some(Box::new(ChecksummedDownloadable::new(&url, &file_path, force_download)));
    }

    // If the lib has multiple urls (like for each OS)
    // We obtain the download info for the OS
    if let Some(DownloadInfo { url, sha1, .. }) = self.get_download_info(os) {
      let downloadable = PreHashedDownloadable::new(&url, &file_path, false, sha1);
      Some(Box::new(downloadable))
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
  pub artifact: DownloadInfo,
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub classifiers: HashMap<String, DownloadInfo>,
}

impl LibraryDownloadInfo {
  pub fn get_download_info(&self, classifier: Option<&str>) -> Option<DownloadInfo> {
    if let Some(classifier) = classifier { self.classifiers.get(classifier).cloned() } else { Some(self.artifact.clone()) }
  }
}
