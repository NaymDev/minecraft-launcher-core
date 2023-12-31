pub mod rule;
pub mod library;
pub mod date;
pub mod artifact;

use std::{ collections::{ HashMap, HashSet }, io::Read, fmt::{ Debug, Display }, path::{ PathBuf, MAIN_SEPARATOR_STR } };

use async_recursion::async_recursion;
use log::info;
use reqwest::Client;
use serde::{ Serialize, Deserialize };
use sha1::{ Digest, Sha1 };

use crate::{ MinecraftLauncherError, download_utils::{ Downloadable, ProxyOptions } };

use self::{ rule::{ Rule, OperatingSystem, FeatureMatcher, RuleAction }, library::Library, date::Date };

use super::{ info::{ ReleaseType, MCVersion, RemoteVersionInfo, VersionInfo }, VersionManager };

const VERSION_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct RawVersionList {
  pub latest: HashMap<ReleaseType, MCVersion>,
  pub versions: Vec<RemoteVersionInfo>,
}

impl RawVersionList {
  pub async fn fetch() -> Result<RawVersionList, reqwest::Error> {
    Client::new().get(VERSION_MANIFEST_URL).send().await?.json::<RawVersionList>().await
  }
}

//

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Sha1Sum([u8; 20]);

impl Sha1Sum {
  pub fn new(value: [u8; 20]) -> Self {
    Self(value)
  }

  pub fn from_reader<T: Read>(value: &mut T) -> Result<Self, Box<dyn std::error::Error>> {
    let mut sha1_hasher = Sha1::new();
    let mut buf = vec![];
    value.read_to_end(&mut buf)?;
    sha1_hasher.update(&buf);
    Ok(Sha1Sum(sha1_hasher.finalize().into()))
  }
}

impl TryFrom<String> for Sha1Sum {
  type Error = String;
  fn try_from(value: String) -> Result<Self, Self::Error> {
    let mut buf = [0u8; 20];
    hex::decode_to_slice(value, &mut buf).map_err(|e| e.to_string())?;
    Ok(Sha1Sum(buf))
  }
}

impl Into<String> for Sha1Sum {
  fn into(self) -> String {
    hex::encode(self.0)
  }
}

impl Debug for Sha1Sum {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", hex::encode(self.0))
  }
}

impl Display for Sha1Sum {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", hex::encode(self.0))
  }
}

//

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentType {
  Game,
  Jvm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum Argument {
  Value(ArgumentValue),
  Object {
    rules: Vec<Rule>,
    value: ArgumentValue,
  },
}

impl Argument {
  pub fn apply(&self, matcher: &impl FeatureMatcher) -> Option<Vec<&String>> {
    if self.applies_to_current_environment(matcher) { Some(self.value()) } else { None }
  }

  pub fn value(&self) -> Vec<&String> {
    match self {
      Argument::Value(value) => value.value(),
      Argument::Object { value, .. } => value.value(),
    }
  }

  pub fn applies_to_current_environment(&self, matcher: &impl FeatureMatcher) -> bool {
    match self {
      Argument::Value(_) => true,
      Argument::Object { rules, .. } => {
        let mut action = RuleAction::Disallow;
        for rule in rules {
          if let Some(applied_action) = rule.get_applied_action(Some(matcher)) {
            action = applied_action;
          }
        }

        action == RuleAction::Allow
      }
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
  String(String),
  List(Vec<String>),
}

impl ArgumentValue {
  pub fn value(&self) -> Vec<&String> {
    match self {
      ArgumentValue::List(list) => list.iter().collect(),
      ArgumentValue::String(string) => vec![string],
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexInfo {
  pub id: String,
  pub sha1: Sha1Sum,
  pub size: i64,
  pub total_size: i64,
  pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DownloadType {
  Client,
  Server,
  WindowsServer,
  ClientMappings,
  ServerMappings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
  pub sha1: Sha1Sum,
  pub size: i64,
  pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionInfo {
  pub component: String,
  pub major_version: i64,
}

impl Default for JavaVersionInfo {
  fn default() -> Self {
    Self {
      component: format!("jre-legacy"),
      major_version: 8,
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LoggingEntry {
  pub argument: String, // "-Dlog4j.configurationFile=${path}"
  pub file: LoggingEntryFile,
  #[serde(rename = "type")]
  pub log_type: String, // "log4j2-xml"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LoggingEntryFile {
  pub id: String, // "client-1.12.xml" ("client-1.7.xml" for 1.10.2)
  pub sha1: Sha1Sum,
  pub size: i64,
  pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LocalVersionInfo {
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

impl VersionInfo for LocalVersionInfo {
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

impl LocalVersionInfo {
  pub fn get_relevant_libraries(&self, matcher: &dyn FeatureMatcher) -> Vec<&Library> {
    self.libraries
      .iter()
      .filter(|lib| lib.applies_to_current_environment(matcher))
      .collect()
  }

  pub fn get_required_downloadables(
    &self,
    os: &OperatingSystem,
    proxy: &ProxyOptions,
    mc_dir: &PathBuf,
    force_download: bool,
    matcher: &impl FeatureMatcher
  ) -> Vec<Box<dyn Downloadable + Send + Sync>> {
    let mut vec = vec![];
    for lib in self.get_relevant_libraries(matcher) {
      let classifier = if !lib.natives.is_empty() {
        if let Some(native) = lib.natives.get(os) {
          Some(native.as_str())
        } else {
          continue;
        }
      } else {
        None
      };

      let mut name = lib.name.clone();
      if let Some(classifier) = classifier {
        name.classifier = Some(classifier.to_string());
      }

      let file = name.get_local_path(&mc_dir.join("libraries"));
      let downloadable = lib.create_download(proxy, &name.get_path_string(), &file, force_download, classifier);
      if let Some(downloadable) = downloadable {
        vec.push(downloadable);
      }
    }
    vec
  }

  pub fn get_required_files(&self, os: &OperatingSystem, matcher: &dyn FeatureMatcher) -> HashSet<String> {
    let mut set = HashSet::new();
    let libraries = self.get_relevant_libraries(matcher);
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

  pub fn applies_to_current_environment(&self, matcher: &impl FeatureMatcher) -> bool {
    if !self.compatibility_rules.is_empty() {
      let mut action = RuleAction::Disallow;
      for rule in &self.compatibility_rules {
        if let Some(applied_action) = rule.get_applied_action(Some(matcher)) {
          action = applied_action;
        }
      }

      action == RuleAction::Allow
    } else {
      true
    }
  }

  pub fn get_classpath(&self, os: &OperatingSystem, mc_dir: &PathBuf, matcher: &impl FeatureMatcher) -> Vec<PathBuf> {
    let mut vec = vec![];
    let libraries = self.get_relevant_libraries(matcher);
    for library in libraries {
      if library.natives.is_empty() {
        vec.push(mc_dir.join("libraries").join(library.get_artifact_path(None).replace("/", MAIN_SEPARATOR_STR)));
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
  ) -> Result<LocalVersionInfo, Box<dyn std::error::Error>> {
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
      Err(MinecraftLauncherError(format!("Circular dependency detected! {} -> [{}]", trace.join(" -> "), self.id.to_string())))?;
    }

    let local_version: LocalVersionInfo = if let Some(local_version) = version_manager.get_local_version(inherits_from) {
      let local_version = local_version.clone();
      if !version_manager.is_up_to_date(&local_version).await {
        version_manager.install_version(inherits_from).await?.clone()
      } else {
        local_version
      }
    } else {
      version_manager.install_version(inherits_from).await?.clone()
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
  pub objects: HashMap<String, AssetObject>,
  #[serde(default)]
  pub map_to_resources: bool,
  #[serde(default, rename="virtual")]
  pub is_virtual: bool
}

impl AssetIndex {
  pub fn get_file_map(&self) -> HashMap<&String, &AssetObject> {
    self.objects.iter().collect()
  }

  pub fn get_unique_objects(&self) -> HashMap<&AssetObject, &String> {
    self.objects
      .iter()
      .map(|(k, v)| (v, k))
      .collect()
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct AssetObject {
  pub hash: Sha1Sum,
  pub size: u64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reconstruct: Option<bool>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub compressed_hash: Option<Sha1Sum>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub compressed_size: Option<u64>,
}

impl AssetObject {
  pub fn has_compressed_alternative(&self) -> bool {
    self.compressed_hash.is_some() && self.compressed_size.is_some()
  }

  pub fn create_path_from_hash(hash: &Sha1Sum) -> String {
    let hash = hash.to_string();
    format!("{}/{}", &hash[0..2], hash)
  }
}

#[cfg(test)]
mod tests {
  use reqwest::Client;
  use serde_json::Value;

  use super::*;

  #[tokio::test]
  async fn test_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let json: Value = Client::new().get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").send().await?.json().await?;
    let versions = json["versions"].as_array().unwrap().to_vec();
    for ver in versions {
      let ver_id = ver["id"].as_str().unwrap();
      let ver = MCVersion::from(ver_id.to_string());
      assert_eq!(ver_id.to_string(), ver.to_string());
    }
    Ok(())
  }
}
