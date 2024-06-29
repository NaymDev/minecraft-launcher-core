use std::{ ffi::OsStr, path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use reqwest::Client;

use crate::json::Sha1Sum;

use super::{ error::HashError, Downloadable, DownloadableMonitor };

pub struct PreHashedDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub expected_hash: Sha1Sum,
  pub monitor: Arc<DownloadableMonitor>,
}

impl PreHashedDownloadable {
  pub fn new(url: &str, target_file: &Path, expected_hash: Sha1Sum) -> Self {
    Self {
      url: url.to_string(),
      target_file: target_file.to_path_buf(),
      start_time: Arc::new(Mutex::new(None)),
      end_time: Arc::new(Mutex::new(None)),

      expected_hash,
      monitor: Arc::new(DownloadableMonitor::new(0, 5242880)),
    }
  }
}

#[async_trait]
impl Downloadable for PreHashedDownloadable {
  fn url(&self) -> &String {
    &self.url
  }

  fn get_target_file(&self) -> &PathBuf {
    &self.target_file
  }

  fn get_status(&self) -> String {
    let file_name = self.get_target_file().file_name().and_then(OsStr::to_str).unwrap_or(self.url());
    format!("Downloading {}", file_name)
  }

  fn get_monitor(&self) -> &Arc<DownloadableMonitor> {
    &self.monitor
  }

  fn get_start_time(&self) -> Option<u64> {
    *self.start_time.lock().unwrap()
  }

  fn set_start_time(&self, start_time: u64) {
    *self.start_time.lock().unwrap() = Some(start_time);
  }

  fn get_end_time(&self) -> Option<u64> {
    *self.end_time.lock().unwrap()
  }

  fn set_end_time(&self, end_time: u64) {
    *self.end_time.lock().unwrap() = Some(end_time);
  }

  async fn get_expected_hash(&self, _: &Client) -> Result<Option<Vec<u8>>, HashError> {
    Ok(Some(self.expected_hash.clone().into()))
  }
}
