use std::sync::Arc;

use download_job::DownloadJob;
use utils::{ get_jar_downloadable, get_library_downloadables, get_asset_downloadables };

use crate::{ json::manifest::VersionManifest, progress_reporter::ProgressReporter };

use super::VersionManager;

pub mod download_job;
pub mod downloadables;
pub mod utils;
pub mod error;

pub struct ClientDownloader {
  pub concurrent_downloads: usize,
  pub max_download_attempts: usize,
  pub reporter: Arc<ProgressReporter>,
}

impl ClientDownloader {
  pub fn new(parallel_downloads: usize, max_download_attempts: usize, reporter: Arc<ProgressReporter>) -> Self {
    Self {
      concurrent_downloads: parallel_downloads,
      max_download_attempts,
      reporter,
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

  pub async fn download_version(&self, local_version: &VersionManifest, version_manager: &VersionManager) -> Result<(), Box<dyn std::error::Error>> {
    let VersionManager { game_dir, env_features, .. } = version_manager;
    let version_job = self
      .create_download_job("Version & Libraries")
      .add_downloadables(get_library_downloadables(game_dir, local_version, env_features, None))
      .add_downloadables(vec![get_jar_downloadable(game_dir, local_version)]);
    let assets_job = self.create_download_job("Resources").add_downloadables(get_asset_downloadables(game_dir, local_version).await?);

    // Download one at a time
    version_job.start().await?;
    assets_job.start().await?;
    Ok(())
  }

  pub fn create_download_job(&self, name: &str) -> DownloadJob {
    DownloadJob::new(name)
      .ignore_failures(false)
      .concurrent_downloads(self.concurrent_downloads as u16)
      .max_download_attempts(self.max_download_attempts as u8)
      .with_progress_reporter(&self.reporter)
  }
}
