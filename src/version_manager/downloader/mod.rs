use std::{ fs::{ self, File }, path::Path };

use download_job::DownloadJob;
use downloadables::{ DownloadError, HashError };
use log::warn;
use md5::Digest;
use progress::ProgressReporter;
use reqwest::Client;
use sha1::Sha1;
use utils::{ get_jar_downloadable, get_library_downloadables, get_asset_downloadables };

use crate::json::{ manifest::{ assets::AssetIndex, VersionManifest }, Sha1Sum };

use super::VersionManager;

pub mod progress;
pub mod download_job;
pub mod downloadables;
pub mod utils;
pub mod error;

pub struct ClientDownloader {
  pub client: Client,
  pub parallel_downloads: Option<usize>,
  pub retries: Option<usize>,
  pub reporter: ProgressReporter,
}

impl ClientDownloader {
  pub fn new(client: Option<Client>, reporter: ProgressReporter, parallel_downloads: Option<usize>, retries: Option<usize>) -> Self {
    Self {
      client: client.unwrap_or(DownloadJob::create_http_client(None).unwrap_or_default()),
      reporter,
      parallel_downloads,
      retries,
    }
  }

  async fn get_asset_index(&self, local_version: &VersionManifest, game_dir: &Path) -> Result<AssetIndex, DownloadError> {
    let index_info = local_version.asset_index.as_ref().ok_or(DownloadError::Other("Asset index not found in version manifest!".into()))?;

    let indexes_dir = game_dir.join("assets").join("indexes");
    let index_file = indexes_dir.join(format!("{}.json", index_info.id));

    if let Ok(mut file) = File::open(&index_file) {
      let sha1 = Sha1Sum::from_reader(&mut file).map_err(HashError::ChecksumFile)?;
      if sha1 != index_info.sha1 {
        warn!("Asset index file is invalid, redownloading");
        fs::remove_file(&index_file).map_err(DownloadError::RemoveFile)?;
      }
    }

    if let Ok(file) = File::open(&index_file) {
      Ok(serde_json::from_reader(file).map_err(|err| DownloadError::Other(Box::new(err)))?)
    } else {
      let response = self.client.get(&index_info.url).send().await?.error_for_status()?;
      let bytes = response.bytes().await?;
      fs::create_dir_all(&indexes_dir).map_err(DownloadError::WriteFile)?;
      fs::write(&index_file, &bytes).map_err(DownloadError::WriteFile)?;

      let mut sha1 = Sha1::new();
      sha1.update(&bytes);
      let actual = Sha1Sum::from(sha1);

      if actual != index_info.sha1 {
        let _ = fs::remove_file(&index_file);
        return Err(DownloadError::ChecksumMismatch { expected: index_info.sha1.as_slice().to_vec(), actual: actual.into() });
      } else {
        Ok(serde_json::from_slice(&bytes).map_err(|err| DownloadError::Other(Box::new(err)))?)
      }
    }
  }

  /// Downloads the specified version of the game along with its libraries and resources.
  ///
  /// This function handles the downloading of game version files and associated assets.
  /// It first downloads the game version and libraries, followed by the game resources.
  ///
  /// # Arguments
  /// * `local_version` - A reference to the `VersionManifest` that specifies the details of the version to download.
  /// * `version_manager` - A reference to the `VersionManager` containing configuration and environment features.
  ///
  /// # Returns
  /// A `Result` which is `Ok` if the downloads complete successfully, or an `Err` with an error box if an error occurs.
  ///
  /// # Errors
  /// This function will return an error if any part of the download process fails.

  pub async fn download_version(&self, local_version: &VersionManifest, version_manager: &VersionManager) -> Result<(), error::Error> {
    let VersionManager { game_dir, env_features, .. } = version_manager;
    let asset_index = self.get_asset_index(local_version, game_dir).await?;

    let mut libs = get_library_downloadables(game_dir, local_version, env_features, None);
    libs.push(get_jar_downloadable(game_dir, local_version));

    let version_job = self.create_download_job("Version & Libraries").add_downloadables(libs);
    let assets_job = self.create_download_job("Resources").add_downloadables(get_asset_downloadables(game_dir, &asset_index));

    // Download one at a time
    version_job.start().await?;
    assets_job.start().await?;
    Ok(())
  }

  pub fn create_download_job(&self, name: &str) -> DownloadJob {
    let mut job = DownloadJob::new(name).with_client(self.client.clone()).ignore_failures(false).with_progress_reporter(&self.reporter);
    if let Some(parallel_downloads) = self.parallel_downloads {
      job = job.with_parallel_downloads(parallel_downloads);
    }

    if let Some(retries) = self.retries {
      job = job.with_retries(retries);
    }
    job
  }
}
