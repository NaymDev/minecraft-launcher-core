use std::{
  path::{ PathBuf, MAIN_SEPARATOR_STR },
  fs::{ read_dir, File, create_dir_all, self },
  collections::HashSet,
  sync::{ Mutex, Arc },
  io::Cursor,
  ops::Deref,
};

use error::LoadVersionError;
use local::LocalVersionInfo;
use log::{ info, warn, error };
use remote::{ RawVersionList, RemoteVersionInfo };
use reqwest::Client;

use crate::{
  download_utils::{ download_job::DownloadJob, AssetDownloadable, Downloadable, EtagDownloadable, PreHashedDownloadable, ProxyOptions },
  json::{
    manifest::{ assets::AssetIndex, download::DownloadType, rule::{ FeatureMatcher, OperatingSystem }, VersionManifest },
    MCVersion,
    VersionInfo,
  },
  MinecraftGameRunner,
  MinecraftLauncherError,
};

pub mod local;
pub mod remote;
pub mod error;

type ArcMutex<T> = Arc<Mutex<T>>;

#[derive(Debug)]
pub struct VersionManager {
  pub game_dir: PathBuf,
  pub feature_matcher: Box<dyn FeatureMatcher + Send + Sync>,
  remote_versions_cache: ArcMutex<Vec<RemoteVersionInfo>>,
  local_versions_cache: ArcMutex<Vec<LocalVersionInfo>>,
}

impl VersionManager {
  pub fn new(game_dir: PathBuf, feature_matcher: Box<dyn FeatureMatcher + Send + Sync>) -> Self {
    Self {
      game_dir,
      feature_matcher,
      remote_versions_cache: Arc::default(),
      local_versions_cache: Arc::default(),
    }
  }

  pub fn get_local_versions(&self) -> Vec<LocalVersionInfo> {
    let mutex_guard = self.local_versions_cache.lock().unwrap();
    mutex_guard.to_vec()
  }

  pub fn get_remote_versions(&self) -> Vec<RemoteVersionInfo> {
    let mutex_guard = self.remote_versions_cache.lock().unwrap();
    mutex_guard.to_vec()
  }

  pub async fn refresh(&self) -> Result<(), Box<dyn std::error::Error>> {
    self.refresh_remote_versions().await?;
    self.refresh_local_versions()?;
    Ok(())
  }

  async fn refresh_remote_versions(&self) -> Result<(), Box<dyn std::error::Error>> {
    let remote_versions_cache = Arc::clone(&self.remote_versions_cache);
    remote_versions_cache.lock().unwrap().clear();
    let RawVersionList { versions, .. } = RawVersionList::fetch().await?;
    remote_versions_cache.lock().unwrap().extend(versions);
    Ok(())
  }

  fn refresh_local_versions(&self) -> Result<(), Box<dyn std::error::Error>> {
    let local_versions_cache = Arc::clone(&self.local_versions_cache);
    local_versions_cache.lock().unwrap().clear();

    let versions_dir = &self.game_dir.join("versions");
    match read_dir(versions_dir) {
      Ok(dir) => {
        let dir_names: Vec<String> = dir
          .filter_map(|entry| entry.ok())
          .filter(|entry| entry.path().is_dir())
          .map(|entry| entry.file_name().to_str().unwrap().to_string())
          .collect();

        for version_id in dir_names {
          info!("Scanning local version versions/{}", &version_id);
          let version_dir = &versions_dir.join(&version_id);
          match LocalVersionInfo::load(&version_dir) {
            Ok(local_version) => {
              local_versions_cache.lock().unwrap().push(local_version);
            }
            Err(LoadVersionError::ManifestNotFound) => {
              warn!("Version file not found! Skipping. (versions/{}/{}.json)", &version_id, &version_id);
            }
            Err(LoadVersionError::ManifestParseError(e)) => {
              warn!("Failed to parse version file! Skipping. (versions/{}/{}.json): {}", &version_id, &version_id, e);
            }
            Err(err) => warn!("Failed to load version: {}", err),
          }
        }
      }
      Err(err) => warn!("Failed to read version directory: {}", err),
    }
    Ok(())
  }

  fn has_all_files(&self, local: &VersionManifest, os: &OperatingSystem) -> bool {
    let required_files = local.get_required_files(os, self.feature_matcher.deref());
    !required_files
      .iter()
      .find(|file| self.game_dir.join(file).is_file())
      .is_none()
  }

  pub fn get_remote_version(&self, version_id: &MCVersion) -> Option<RemoteVersionInfo> {
    self.remote_versions_cache
      .lock()
      .unwrap()
      .iter()
      .find(|v| v.get_id() == version_id)
      .cloned()
  }

  pub fn get_local_version(&self, version_id: &MCVersion) -> Option<LocalVersionInfo> {
    self.local_versions_cache
      .lock()
      .unwrap()
      .iter()
      .find(|v| v.get_id() == version_id)
      .cloned()
  }

  pub async fn is_up_to_date(&self, local_version: &VersionManifest) -> bool {
    if let Some(remote_version) = self.get_remote_version(local_version.get_id()) {
      if remote_version.get_updated_time().inner() > local_version.get_updated_time().inner() {
        return false;
      }
      let resolved = match local_version.resolve(self, HashSet::new()).await {
        Ok(resolved) => resolved,
        Err(_) => {
          error!("Failed to resolve version {}", local_version.get_id().to_string());
          local_version.clone()
        }
      };

      return self.has_all_files(&resolved, &OperatingSystem::get_current_platform());
    } else {
      true
    }
  }

  pub async fn install_version(&self, version_id: &MCVersion) -> Result<VersionManifest, Box<dyn std::error::Error>> {
    let remote_version = &self
      .get_remote_version(version_id)
      .ok_or(MinecraftLauncherError(format!("Version not found in remote list: {}", &version_id.to_string())))?;

    let version_manifest = remote_version.fetch().await?;
    let target_dir = &self.game_dir.join("versions").join(&version_manifest.get_id().to_string());
    create_dir_all(&target_dir)?;
    let target_json = target_dir.join(format!("{}.json", &version_manifest.get_id().to_string()));
    serde_json::to_writer_pretty(&File::create(&target_json)?, &version_manifest)?;

    let local_version_info = LocalVersionInfo::from_manifest(&target_json)?;
    self.local_versions_cache.lock().unwrap().push(local_version_info);
    Ok(version_manifest)
  }

  pub fn download_version(
    &self,
    game_runner: &MinecraftGameRunner,
    local_version: &VersionManifest,
    download_job: &mut DownloadJob
  ) -> Result<(), Box<dyn std::error::Error>> {
    download_job.add_downloadables(
      local_version.get_required_downloadables(
        &OperatingSystem::get_current_platform(),
        &game_runner.options.proxy,
        &game_runner.options.game_dir,
        false,
        game_runner.feature_matcher.deref()
      )
    );
    let jar_id = local_version.get_jar().to_string();
    let jar_path = format!("versions/{}/{}.jar", &jar_id, &jar_id);
    let jar_file_path = game_runner.options.game_dir.join(&jar_path.replace("/", MAIN_SEPARATOR_STR));

    let info = local_version.get_download_url(DownloadType::Client);
    let http_client = game_runner.options.proxy.create_http_client();
    if let Some(info) = info {
      download_job.add_downloadables(vec![Box::new(PreHashedDownloadable::new(http_client, &info.url, &jar_file_path, false, info.sha1.clone()))]);
    } else {
      let url = format!("https://s3.amazonaws.com/Minecraft.Download/{jar_path}");
      download_job.add_downloadables(vec![Box::new(EtagDownloadable::new(http_client, &url, &jar_file_path, false))]);
    }

    Ok(())
  }

  pub async fn get_resource_files(
    &self,
    proxy: &ProxyOptions,
    game_dir: &PathBuf,
    local_version: &VersionManifest
  ) -> Result<Vec<Box<dyn Downloadable + Send + Sync>>, Box<dyn std::error::Error>> {
    let assets_dir = game_dir.join("assets");
    let objects_dir = assets_dir.join("objects");
    let indexes_dir = assets_dir.join("indexes");

    let mut vec: Vec<Box<dyn Downloadable + Send + Sync>> = vec![];

    let index_info = local_version.asset_index.as_ref().unwrap();
    let index_file = indexes_dir.join(format!("{}.json", index_info.id));

    let url = &index_info.url;
    let bytes = Client::new().get(url).send().await?.bytes().await?;
    create_dir_all(indexes_dir)?;
    fs::write(&index_file, &bytes)?;
    let asset_index: AssetIndex = serde_json::from_reader(&mut Cursor::new(&bytes))?;
    let objects = asset_index.get_unique_objects();
    for (obj, value) in objects {
      // let hash = obj.hash.to_string();
      let downloadable = Box::new(
        AssetDownloadable::new(proxy.create_http_client(), value, obj, "https://resources.download.minecraft.net/", &objects_dir)
      );
      downloadable.monitor.set_total(obj.size as usize);
      vec.push(downloadable);
    }

    Ok(vec)
  }
}

#[cfg(test)]
mod tests {
  use std::env::temp_dir;

  use simple_logger::SimpleLogger;

  use crate::json::manifest::rule::RuleFeatureType;

  use super::*;

  struct TestFeatureMatcher;

  impl FeatureMatcher for TestFeatureMatcher {
    fn has_feature(&self, _feature_type: &RuleFeatureType, _value: &serde_json::Value) -> bool {
      false
    }
  }

  #[tokio::test]
  async fn test_version_manager() -> Result<(), Box<dyn std::error::Error>> {
    SimpleLogger::new().init().unwrap();
    let mut version_manager = VersionManager::new(temp_dir().join(".minecraft-test-rust"), Box::new(TestFeatureMatcher));
    version_manager.refresh().await?;
    info!("{:#?}", version_manager.local_versions_cache);
    let local = version_manager.get_local_version(&MCVersion::from("1.20.1-forge-47.2.0".to_string()));
    if let Some(local) = local {
      let resolved = local.clone().load_manifest()?.resolve(&mut version_manager, HashSet::new()).await?;
      info!("Resolved: {:#?}", resolved);
    }
    Ok(())
  }
}
