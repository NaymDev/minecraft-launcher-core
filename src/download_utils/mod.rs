pub mod download_job;

use std::sync::{ Arc, Mutex };
use crate::progress_reporter::ProgressReporter;

pub mod downloadables;
pub mod error;

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
