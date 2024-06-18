use std::{ fs::create_dir_all, path::PathBuf, sync::Arc };

use async_trait::async_trait;
use log::info;
use reqwest::Client;

use super::{ error::Error, DownloadableMonitor };

mod checksummed;
mod prehashed;
mod etag;
mod asset;

pub use checksummed::ChecksummedDownloadable;
pub use prehashed::PreHashedDownloadable;
pub use etag::EtagDownloadable;
pub use asset::{ AssetDownloadable, AssetDownloadableStatus };

#[async_trait]
pub trait Downloadable: Send + Sync {
  fn url(&self) -> &String;
  fn get_target_file(&self) -> &PathBuf;
  fn force_download(&self) -> bool;
  fn get_attempts(&self) -> usize;

  fn get_status(&self) -> String;
  fn get_monitor(&self) -> &Arc<DownloadableMonitor>;

  fn get_start_time(&self) -> Option<u64>;
  fn set_start_time(&self, start_time: u64);
  fn get_end_time(&self) -> Option<u64>;
  fn set_end_time(&self, end_time: u64);

  fn ensure_file_writable(&self, file: &PathBuf) -> Result<(), Error> {
    if let Some(parent) = file.parent() {
      if !parent.is_dir() {
        info!("Making directory {}", parent.display());
        create_dir_all(parent).map_err(Error::PrepareFolderError)?;
      }
    }

    Ok(())
  }

  async fn download(&self, client: &Client) -> Result<(), Error>;
}
