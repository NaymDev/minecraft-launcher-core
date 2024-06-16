use std::{
  collections::HashSet,
  fs::{ self, create_dir_all, read_dir, File },
  io::Cursor,
  path::{ PathBuf, MAIN_SEPARATOR_STR },
  sync::{ Arc, Mutex },
};

use error::{ InstallVersionError, LoadVersionError };
use log::{ error, info, warn };
use remote::{ RawVersionList, RemoteVersionInfo };
use reqwest::Client;

use crate::{
  bootstrap::GameBootstrap,
  download_utils::{ download_job::DownloadJob, AssetDownloadable, Downloadable, EtagDownloadable, PreHashedDownloadable, ProxyOptions },
  json::{
    manifest::{ assets::AssetIndex, download::DownloadType, rule::OperatingSystem, VersionManifest },
    EnvironmentFeatures,
    MCVersion,
    VersionInfo,
  },
};

pub mod remote;
pub mod error;

#[derive(Debug)]
pub struct VersionManager {
  pub game_dir: PathBuf,
  pub env_features: EnvironmentFeatures,

  local_cache: Arc<Mutex<Vec<MCVersion>>>,
  remote_cache: Option<RawVersionList>,
}

impl VersionManager {
  pub async fn new(game_dir: PathBuf, env_features: EnvironmentFeatures) -> Result<Self, LoadVersionError> {
    let mut version_manager = Self { game_dir, env_features, local_cache: Arc::default(), remote_cache: None };
    version_manager.refresh().await?;
    Ok(version_manager)
  }

  fn versions_dir(&self) -> PathBuf {
    self.game_dir.join("versions")
  }

  pub fn installed_versions(&self) -> Vec<MCVersion> {
    if let Ok(local_cache) = self.local_cache.try_lock() {
      return local_cache.to_vec();
    } else {
      return vec![];
    }
  }

  pub fn remote_versions(&self) -> Vec<&MCVersion> {
    self.remote_cache
      .iter()
      .map(|raw| &raw.versions)
      .flatten()
      .map(|v| v.get_id())
      .collect()
  }

  pub fn get_remote_version(&self, version_id: &MCVersion) -> Option<&RemoteVersionInfo> {
    self.remote_cache
      .iter()
      .map(|raw| &raw.versions)
      .flatten()
      .find(|v| v.get_id() == version_id)
  }

  /// Retrieves the local version information based on the provided version identifier.
  ///
  /// This function searches through a cached list of local versions, attempting to find
  /// a version that matches the given `version_id`. If found, it returns a clone of the
  /// `LocalVersionInfo` associated with that version.
  ///
  /// # Arguments
  /// * `version_id` - A reference to the `MCVersion` identifier for which the local version info is sought.
  ///
  /// # Returns
  /// An `Option<LocalVersionInfo>` which is `Some` if the version is found, otherwise `None`.
  ///
  /// # Panics
  /// This function will panic if the lock on the cache cannot be acquired.
  pub fn get_installed_version(&self, version_id: &MCVersion) -> Result<VersionManifest, LoadVersionError> {
    let installed_versions = self.installed_versions();
    if !installed_versions.contains(version_id) {
      return Err(LoadVersionError::VersionNotFound(version_id.to_string()));
    }
    self.load_manifest(version_id)
  }
}

impl VersionManager {
  pub async fn refresh(&mut self) -> Result<(), LoadVersionError> {
    self.remote_cache.replace(RawVersionList::fetch().await?);
    self.refresh_local_versions()?;
    Ok(())
  }

  fn refresh_local_versions(&self) -> Result<(), LoadVersionError> {
    let local_cache = Arc::clone(&self.local_cache);
    local_cache.lock().unwrap().clear();

    let versions_dir = &self.game_dir.join("versions");
    match read_dir(versions_dir) {
      Ok(dir) => {
        let dir_names: Vec<String> = dir
          .filter_map(|entry| entry.ok())
          .filter(|entry| entry.path().is_dir())
          .map(|entry| entry.file_name().into_string())
          .flatten()
          .collect();

        let mut versions = vec![];
        for version_id in dir_names {
          info!("Scanning local version versions/{}", &version_id);
          let version_id = MCVersion::from(version_id);
          match self.load_manifest(&version_id) {
            Ok(_) => {
              versions.push(version_id);
            }
            Err(LoadVersionError::ManifestNotFound) => {
              let version_id = version_id.to_string();
              warn!("Version file not found! Skipping. (versions/{}/{}.json)", &version_id, &version_id);
            }
            Err(LoadVersionError::ManifestParseError(e)) => {
              let version_id = version_id.to_string();
              warn!("Failed to parse version file! Skipping. (versions/{}/{}.json): {}", &version_id, &version_id, e);
            }
            Err(err) => warn!("Failed to load version: {}", err),
          }
        }
        local_cache.lock().unwrap().extend(versions);
      }
      Err(err) => warn!("Failed to read version directory: {}", err),
    }
    Ok(())
  }

  fn load_manifest(&self, version_id: &MCVersion) -> Result<VersionManifest, LoadVersionError> {
    let version_id = version_id.to_string();
    let manifest_path = self.versions_dir().join(&version_id).join(format!("{}.json", &version_id));
    if !manifest_path.is_file() {
      return Err(LoadVersionError::ManifestNotFound);
    }
    let manifest_file = File::open(&manifest_path)?;
    return Ok(serde_json::from_reader(manifest_file)?);
  }
}

/* Version Download Functions */
// Install Version (downloads manifest only)
impl VersionManager {
  pub async fn install_version_by_id(&self, version_id: &MCVersion) -> Result<VersionManifest, InstallVersionError> {
    if let Some(remote_version) = self.get_remote_version(version_id) {
      return self.install_version(&remote_version).await;
    }
    Err(InstallVersionError::VersionNotFound(version_id.to_string()))
  }

  pub async fn install_version(&self, remote_version: &RemoteVersionInfo) -> Result<VersionManifest, InstallVersionError> {
    let version_manifest = remote_version.fetch().await?;
    let version_id = version_manifest.get_id().to_string();

    let target_dir = self.versions_dir().join(&version_id);
    create_dir_all(&target_dir)?;
    let target_json = target_dir.join(format!("{}.json", &version_id));
    serde_json::to_writer_pretty(&File::create(&target_json)?, &version_manifest)?;

    if let Ok(mut local_cache) = self.local_cache.lock() {
      local_cache.push(version_manifest.get_id().clone());
    }
    Ok(version_manifest)
  }
}

// Assets and Libraries
impl VersionManager {
  fn has_all_files(&self, local: &VersionManifest, os: &OperatingSystem) -> bool {
    let required_files = local.get_required_files(os, &self.env_features);
    required_files.iter().all(|file| self.game_dir.join(file).is_file())
  }

  pub async fn is_up_to_date(&self, version_manifest: &VersionManifest) -> bool {
    if let Some(remote_version) = self.get_remote_version(version_manifest.get_id()) {
      if remote_version.get_updated_time().inner() > version_manifest.get_updated_time().inner() {
        return false;
      }

      match version_manifest.resolve(self, HashSet::new()).await {
        Ok(resolved) => {
          return self.has_all_files(&resolved, &OperatingSystem::get_current_platform());
        }
        Err(_) => {
          error!("Failed to resolve version {}", version_manifest.get_id().to_string());
          return self.has_all_files(version_manifest, &OperatingSystem::get_current_platform());
        }
      }
    } else {
      true
    }
  }

  pub fn get_version_downloadables(&self, proxy: &ProxyOptions, local_version: &VersionManifest) -> Vec<Box<dyn Downloadable + Send + Sync>> {
    let mut downloadables = local_version.get_required_downloadables(
      &OperatingSystem::get_current_platform(),
      proxy,
      &self.game_dir,
      false,
      &self.env_features
    );

    let jar_id = local_version.get_jar().to_string();
    let jar_path = format!("versions/{}/{}.jar", &jar_id, &jar_id);
    let jar_file_path = &self.game_dir.join(&jar_path.replace("/", MAIN_SEPARATOR_STR));

    let info = local_version.get_download_url(DownloadType::Client);
    let http_client = proxy.create_http_client();

    let downloadable: Box<dyn Downloadable + Send + Sync> = if let Some(info) = info {
      Box::new(PreHashedDownloadable::new(http_client, &info.url, &jar_file_path, false, info.sha1.clone()))
    } else {
      let url = format!("https://s3.amazonaws.com/Minecraft.Download/{jar_path}");
      Box::new(EtagDownloadable::new(http_client, &url, &jar_file_path, false))
    };
    downloadables.push(downloadable);

    downloadables
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

  use super::*;

  #[tokio::test]
  async fn test_version_manager() -> Result<(), Box<dyn std::error::Error>> {
    SimpleLogger::new().init().unwrap();
    let mut version_manager = VersionManager::new(temp_dir().join(".minecraft-test-rust"), EnvironmentFeatures::default()).await?;
    info!("{:#?}", version_manager.local_cache);
    let local = version_manager.get_installed_version(&MCVersion::from("1.20.1-forge-47.2.0".to_string()));
    if let Ok(local) = local {
      let resolved = local.resolve(&mut version_manager, HashSet::new()).await?;
      info!("Resolved: {:#?}", resolved);
    }
    Ok(())
  }
}
