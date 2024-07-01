use std::{ collections::{ HashMap, HashSet }, fs::{ self, create_dir_all, read_dir, File }, path::{ Path, PathBuf }, sync::Arc };

use downloader::{ download_job::DownloadJob, progress::ProgressReporter, ClientDownloader };
use error::{ InstallVersionError, LoadVersionError, ResolveManifestError };
use log::{ error, info, warn };
use remote::{ RawVersionList, RemoteVersionInfo };
use reqwest::Client;
use utils::resolve;

use crate::json::{ manifest::{ rule::OperatingSystem, VersionManifest }, EnvironmentFeatures, MCVersion, VersionInfo };

pub mod downloader;
pub mod remote;
pub mod error;

mod utils;

#[derive(Debug)]
pub struct VersionManager {
  pub game_dir: PathBuf,
  pub env_features: EnvironmentFeatures,
  pub client: Client,

  local_cache: Vec<MCVersion>,
  remote_cache: Option<RawVersionList>,

  resolved_versions_cache: HashMap<MCVersion, VersionManifest>,
}

impl VersionManager {
  /// Create an empty version manager
  /// You must call `VersionManager::refresh` before using it
  pub fn new(game_dir: &Path, env_features: &EnvironmentFeatures, client: Option<Client>) -> Self {
    Self {
      game_dir: game_dir.to_path_buf(),
      env_features: env_features.clone(),
      client: client.unwrap_or(DownloadJob::create_http_client(None).unwrap_or_default()),

      local_cache: vec![],
      remote_cache: None,
      resolved_versions_cache: HashMap::new(),
    }
  }

  /// Loads the version manager with the provided game directory and environment features.
  /// Creates the version manager and refreshes it
  pub async fn load(game_dir: &Path, env_features: &EnvironmentFeatures, client: Option<Client>) -> Result<Self, LoadVersionError> {
    let mut version_manager = Self::new(game_dir, env_features, client);
    version_manager.refresh().await?;
    Ok(version_manager)
  }

  fn versions_dir(&self) -> PathBuf {
    self.game_dir.join("versions")
  }

  pub fn installed_versions(&self) -> Vec<&MCVersion> {
    self.local_cache.iter().collect()
  }

  pub fn remote_versions(&self) -> Vec<&MCVersion> {
    self.remote_cache
      .iter()
      .flat_map(|raw| &raw.versions)
      .map(|v| v.get_id())
      .collect()
  }

  pub fn get_remote_version(&self, version_id: &MCVersion) -> Option<&RemoteVersionInfo> {
    self.remote_cache
      .iter()
      .flat_map(|raw| &raw.versions)
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
    if !installed_versions.contains(&version_id) {
      return Err(LoadVersionError::VersionNotFound(version_id.to_string()));
    }
    self.load_manifest(version_id)
  }

  pub fn get_resolved_version_cache(&self, version_id: &MCVersion) -> Option<&VersionManifest> {
    self.resolved_versions_cache.get(version_id)
  }
}

impl VersionManager {
  pub async fn refresh(&mut self) -> Result<(), LoadVersionError> {
    self.remote_cache.replace(RawVersionList::fetch(&self.client).await?);
    self.refresh_local_versions()?;
    Ok(())
  }

  fn refresh_local_versions(&mut self) -> Result<(), LoadVersionError> {
    self.local_cache.clear();
    self.resolved_versions_cache.clear();

    let versions_dir = &self.game_dir.join("versions");
    match read_dir(versions_dir) {
      Ok(dir) => {
        let dir_names: Vec<String> = dir
          .filter_map(|entry| entry.ok())
          .filter(|entry| entry.path().is_dir())
          .flat_map(|entry| entry.file_name().into_string())
          .collect();

        let mut versions = vec![];
        for version_id in dir_names {
          info!("Scanning local version versions/{}", &version_id);
          let version_id = MCVersion::from(version_id);
          match self.load_manifest(&version_id) {
            Ok(_) => versions.push(version_id),
            Err(LoadVersionError::ManifestNotFound) => {
              warn!("Version file not found! Skipping. (versions/{}/{}.json)", &version_id, &version_id);
            }
            Err(err) => {
              warn!("Failed to parse version file! Skipping. (versions/{}/{}.json): {}", &version_id, &version_id, err);
            }
          }
        }
        self.local_cache.append(&mut versions);
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
    Ok(serde_json::from_reader(manifest_file)?)
  }
}

/* Version Download Functions */
// Install Version (downloads manifest only)
impl VersionManager {
  /// Resolves the full version manifest for a given Minecraft version, optionally updating it if not up-to-date.
  ///
  /// If enabled, the function will first check if the specified version was already resolved in the cache. If so, it will return a clone of the cached manifest.
  /// Otherwise, the function first checks if the specified version is already installed. If it is, it uses the installed version.
  /// If not, it installs the version. If the `update_if_necessary` flag is set and the installed version is not up-to-date,
  /// it updates the version by reinstalling it.
  ///
  /// # Arguments
  /// * `version_id` - A reference to the version identifier for which the manifest needs to be resolved.
  /// * `update_if_necessary` - A boolean flag indicating whether to update the version if it is not up-to-date.
  /// * `ignore_cache` - A boolean flag indicating whether to ignore the cache of resolved versions.
  ///
  /// # Returns
  /// This function returns a `Result` containing either the fully resolved `VersionManifest` or an error.
  ///
  /// # Errors
  /// This function can return an error if there is a problem with installing the version, checking its update status,
  /// or resolving inheritances.
  ///
  /// # Examples
  /// ```ignore
  /// let mut resolver = VersionResolver::new();
  /// let version_id = MCVersion::new("1.16.4");
  /// let manifest = resolver.resolve(&version_id, true, true).await?;
  /// ```
  pub async fn resolve_local_version(
    &mut self,
    version_id: &MCVersion,
    update_if_necessary: bool,
    ignore_cache: bool
  ) -> Result<VersionManifest, ResolveManifestError> {
    if !ignore_cache {
      if let Some(manifest) = self.get_resolved_version_cache(version_id) {
        return Ok(manifest.clone());
      }
    }

    let mut manifest = if let Ok(manifest) = self.get_installed_version(version_id) {
      manifest
    } else {
      self.install_version_by_id(version_id).await?
    };

    if update_if_necessary && !self.is_up_to_date(&manifest).await {
      manifest = self.install_version_by_id(version_id).await?;
    }

    let resolved = self.resolve_inheritances(manifest).await?;
    self.resolved_versions_cache.insert(resolved.id.clone(), resolved.clone());
    Ok(resolved)
  }

  pub async fn install_version_by_id(&mut self, version_id: &MCVersion) -> Result<VersionManifest, InstallVersionError> {
    if let Some(remote_version) = self.get_remote_version(version_id) {
      return self.install_version(&remote_version.clone()).await;
    }
    Err(InstallVersionError::VersionNotFound(version_id.to_string()))
  }

  pub async fn install_version(&mut self, remote_version: &RemoteVersionInfo) -> Result<VersionManifest, InstallVersionError> {
    let version_id = remote_version.get_id().to_string();
    let target_dir = self.versions_dir().join(&version_id);
    let target_json = target_dir.join(format!("{}.json", &version_id));

    let bytes = self.client.get(remote_version.get_url()).send().await?.error_for_status()?.bytes().await?;
    create_dir_all(&target_dir)?;
    fs::write(target_json, &bytes)?;
    let version_manifest: VersionManifest = serde_json::from_slice(&bytes)?;

    self.local_cache.push(version_manifest.get_id().clone());
    Ok(version_manifest)
  }
}

impl VersionManager {
  pub async fn resolve_inheritances(&mut self, version_manifest: VersionManifest) -> Result<VersionManifest, ResolveManifestError> {
    resolve(version_manifest, self, &mut HashSet::new()).await
  }

  pub async fn is_up_to_date(&mut self, version_manifest: &VersionManifest) -> bool {
    if let Some(remote_version) = self.get_remote_version(version_manifest.get_id()) {
      if remote_version.get_updated_time().inner() > version_manifest.get_updated_time().inner() {
        return false;
      }

      match self.resolve_inheritances(version_manifest.clone()).await {
        Ok(resolved) => { self.has_all_files(&resolved, &OperatingSystem::get_current_platform()) }
        Err(_) => {
          error!("Failed to resolve version {}", version_manifest.get_id().to_string());
          self.has_all_files(version_manifest, &OperatingSystem::get_current_platform())
        }
      }
    } else {
      true
    }
  }

  pub async fn download_required_files(
    &self,
    version_manifest: &VersionManifest,
    progress_reporter: &ProgressReporter,
    parallel_downloads: Option<usize>,
    retries: Option<usize>
  ) -> Result<(), downloader::error::Error> {
    let downloader = ClientDownloader::new(Some(self.client.clone()), Arc::clone(progress_reporter), parallel_downloads, retries);
    downloader.download_version(version_manifest, self).await
  }
}

// Assets and Libraries
impl VersionManager {
  fn has_all_files(&self, local: &VersionManifest, os: &OperatingSystem) -> bool {
    let required_files = local.get_required_files(os, &self.env_features);
    required_files.iter().all(|file| self.game_dir.join(file).is_file())
  }
}
