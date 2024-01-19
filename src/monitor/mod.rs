use std::fmt::Debug;

use serde::{ Serialize, Deserialize };

pub trait Monitor: Debug {
  fn set_download_progress(&mut self, download_progress: DownloadProgress);
  fn hide_download_progress(&mut self);

  fn get_download_progress(&self) -> Option<&DownloadProgress>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadProgress {
  status: String,
  current: u64,
  total: u64,
}

impl DownloadProgress {
  pub fn new(status: &str, current: u64, total: u64) -> Self {
    Self {
      status: status.to_string(),
      current,
      total,
    }
  }

  pub fn get_status(&self) -> &str {
    &self.status
  }

  pub fn get_current(&self) -> u64 {
    self.current
  }

  pub fn get_total(&self) -> u64 {
    self.total
  }

  pub fn get_percentage(&self) -> f32 {
    (self.current as f32) / (self.total as f32)
  }
}

//

#[derive(Debug)]
pub struct MockMonitor(Option<DownloadProgress>);

impl MockMonitor {
  pub fn new() -> Self {
    Self(None)
  }
}

impl Monitor for MockMonitor {
  fn set_download_progress(&mut self, download_progress: DownloadProgress) {
    self.0.replace(download_progress);
  }

  fn hide_download_progress(&mut self) {
    self.0.take();
  }

  fn get_download_progress(&self) -> Option<&DownloadProgress> {
    self.0.as_ref()
  }
}

impl Drop for MockMonitor {
  fn drop(&mut self) {
    self.hide_download_progress();
  }
}
