use std::{ collections::{ HashMap, HashSet }, path::{ Path, PathBuf, MAIN_SEPARATOR_STR } };

use argument::{ Argument, ArgumentType };
use assets::AssetIndexInfo;
use async_recursion::async_recursion;
use download::{ DownloadInfo, DownloadType };
use java::JavaVersionInfo;
use library::Library;
use logging::LoggingEntry;
use rule::{ OperatingSystem, Rule, RuleAction };
use serde::{ Deserialize, Serialize };

use crate::{ bootstrap::MinecraftLauncherError, version_manager::VersionManager };

use super::{ Date, EnvironmentFeatures, MCVersion, ReleaseType, VersionInfo };

pub mod argument;
pub mod assets;
pub mod download;
pub mod java;
pub mod logging;
pub mod rule;
pub mod library;
pub mod artifact;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub arguments: HashMap<ArgumentType, Vec<Argument>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub minecraft_arguments: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub asset_index: Option<AssetIndexInfo>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  assets: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  compatibility_rules: Vec<Rule>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  compliance_level: Option<u8>,
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  downloads: HashMap<DownloadType, DownloadInfo>,
  id: MCVersion,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  inherits_from: Option<MCVersion>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  java_version: Option<JavaVersionInfo>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  libraries: Vec<Library>,
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  logging: HashMap<DownloadType, LoggingEntry>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  main_class: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  jar: Option<MCVersion>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  minimum_launcher_version: Option<u32>,
  release_time: Date,
  #[serde(rename = "time")]
  updated_time: Date,
  #[serde(rename = "type")]
  release_type: ReleaseType,
}

impl VersionManifest {
  pub fn get_relevant_libraries(&self, env_features: &EnvironmentFeatures) -> Vec<&Library> {
    self.libraries
      .iter()
      .filter(|lib| lib.applies_to_current_environment(env_features))
      .collect()
  }

  pub fn get_required_files(&self, os: &OperatingSystem, env_features: &EnvironmentFeatures) -> HashSet<String> {
    let mut set = HashSet::new();
    let libraries = self.get_relevant_libraries(env_features);
    for library in libraries {
      if !library.natives.is_empty() {
        if let Some(native) = library.natives.get(os) {
          set.insert(format!("libraries/{}", library.get_artifact_path(Some(native.as_str()))));
        }
      } else {
        set.insert(format!("libraries/{}", library.get_artifact_path(None)));
      }
    }
    set
  }

  pub fn get_jar(&self) -> &MCVersion {
    self.jar.as_ref().unwrap_or(self.get_id())
  }

  pub fn get_main_class(&self) -> &String {
    self.main_class.as_ref().unwrap()
  }

  pub fn get_download_url(&self, download_type: DownloadType) -> Option<&DownloadInfo> {
    self.downloads.get(&download_type)
  }

  pub fn applies_to_current_environment(&self, env_features: &EnvironmentFeatures) -> bool {
    if self.compatibility_rules.is_empty() {
      return true;
    }

    let mut action = RuleAction::Disallow;
    for rule in &self.compatibility_rules {
      if let Some(applied_action) = rule.get_applied_action(env_features) {
        action = applied_action;
      }
    }

    action == RuleAction::Allow
  }

  pub fn get_classpath(&self, _os: &OperatingSystem, mc_dir: &Path, env_features: &EnvironmentFeatures) -> Vec<PathBuf> {
    let mut vec = vec![];
    let libraries = self.get_relevant_libraries(env_features);
    for library in libraries {
      if library.natives.is_empty() {
        vec.push(mc_dir.join("libraries").join(library.get_artifact_path(None).replace('/', MAIN_SEPARATOR_STR)));
      }
    }

    let jar_id = self.get_jar().to_string();
    vec.push(mc_dir.join("versions").join(&jar_id).join(format!("{jar_id}.jar")));
    vec
  }

  #[async_recursion]
  pub async fn resolve(
    &self,
    version_manager: &VersionManager,
    mut inheritance_trace: HashSet<&'async_recursion MCVersion>
  ) -> Result<VersionManifest, Box<dyn std::error::Error>> {
    if self.inherits_from.as_ref().is_none() {
      return Ok(self.clone());
    }
    let inherits_from = self.inherits_from.as_ref().unwrap();
    if !inheritance_trace.insert(&self.id) {
      let mut trace = inheritance_trace
        .iter()
        .map(|ver| ver.to_string())
        .collect::<Vec<_>>();
      trace.reverse();
      Err(MinecraftLauncherError(format!("Circular dependency detected! {} -> [{}]", trace.join(" -> "), self.id)))?;
    }

    let local_version: VersionManifest = if let Ok(local_version) = version_manager.get_installed_version(inherits_from) {
      if !version_manager.is_up_to_date(&local_version).await {
        version_manager.install_version_by_id(inherits_from).await?.clone()
      } else {
        local_version
      }
    } else {
      version_manager.install_version_by_id(inherits_from).await?.clone()
    };

    let mut local_version = local_version.resolve(version_manager, inheritance_trace).await?;
    local_version.inherits_from = None;
    local_version.id = self.id.clone();
    local_version.updated_time = self.updated_time.clone();
    local_version.release_time = self.release_time.clone();
    local_version.release_type = self.release_type.clone();

    if let Some(minecraft_arguments) = &self.minecraft_arguments {
      local_version.minecraft_arguments = Some(minecraft_arguments.clone());
    }

    if let Some(main_class) = &self.main_class {
      local_version.main_class = Some(main_class.clone());
    }

    if let Some(assets) = &self.assets {
      local_version.assets = Some(assets.clone());
    }

    if let Some(jar) = &self.jar {
      local_version.jar = Some(jar.clone());
    }

    if let Some(asset_index) = &self.asset_index {
      local_version.asset_index = Some(asset_index.clone());
    }

    if !self.libraries.is_empty() {
      // local_version.libraries = self.libraries.clone();
      let mut new_libraries = vec![];
      new_libraries.extend(self.libraries.clone());
      new_libraries.extend(local_version.libraries);

      local_version.libraries = new_libraries;
    }

    if !self.arguments.is_empty() {
      // local_version.arguments = self.arguments.clone();
      for (arg_type, args) in &self.arguments {
        if let Some(vec) = local_version.arguments.get_mut(arg_type) {
          vec.extend(args.clone());
        } else {
          local_version.arguments.insert(arg_type.clone(), args.clone());
        }
      }
    }

    if !self.compatibility_rules.is_empty() {
      // local_version.compatibility_rules = self.compatibility_rules.clone();
      local_version.compatibility_rules.extend(self.compatibility_rules.clone());
    }

    if let Some(java_version) = &self.java_version {
      local_version.java_version = Some(java_version.clone());
    }

    Ok(local_version)
  }
}

impl VersionInfo for VersionManifest {
  fn get_id(&self) -> &MCVersion {
    &self.id
  }

  fn get_type(&self) -> &ReleaseType {
    &self.release_type
  }

  fn get_updated_time(&self) -> &Date {
    &self.updated_time
  }

  fn get_release_time(&self) -> &Date {
    &self.release_time
  }
}
