use std::{ fs::create_dir_all, path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use log::info;
use reqwest::Client;

use super::{ error::Error, progress_reporter::ProgressReporter };

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

  fn ensure_file_writable(&self, file: &Path) -> Result<(), Error> {
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

pub struct DownloadableMonitor {
  current: Mutex<usize>,
  total: Mutex<usize>,
  reporter: Mutex<Arc<ProgressReporter>>,
}

impl DownloadableMonitor {
  pub fn new(current: usize, total: usize) -> Self {
    Self {
      current: Mutex::new(current),
      total: Mutex::new(total),
      reporter: Mutex::new(Arc::new(ProgressReporter::new(|_| {}))),
    }
  }

  pub fn get_current(&self) -> usize {
    *self.current.lock().unwrap()
  }

  pub fn get_total(&self) -> usize {
    *self.total.lock().unwrap()
  }

  pub fn set_current(&self, current: usize) {
    *self.current.lock().unwrap() = current;
    self.reporter
      .lock()
      .unwrap()
      .set_progress(current as u32);
  }

  pub fn set_total(&self, total: usize) {
    *self.total.lock().unwrap() = total;
    self.reporter
      .lock()
      .unwrap()
      .set_total(total as u32);
  }

  pub fn set_reporter(&self, reporter: Arc<ProgressReporter>) {
    *self.reporter.lock().unwrap() = reporter;
    // TODO: fire update?
  }
}
