use std::{ sync::{ Arc, Mutex }, collections::VecDeque };

use chrono::Utc;
use futures::future::join_all;
use log::{ info, error, warn };

use crate::MinecraftLauncherError;

use super::Downloadable;

type DownloadableSync = Box<dyn Downloadable + Send + Sync>;

pub struct DownloadJob {
  name: String,
  // all_files: Vec<Box<dyn Downloadable + Send + Sync>>,
  remaining_files: Arc<Mutex<VecDeque<DownloadableSync>>>,
  failures: Arc<Mutex<Vec<DownloadableSync>>>,
  ignore_failures: bool,
  max_pool_size: u16,
  max_download_attempts: u8,
}

impl DownloadJob {
  pub fn new(name: &str, ignore_failures: bool, max_pool_size: u16, max_download_attempts: u8) -> Self {
    Self {
      name: name.to_string(),
      // all_files: vec![],
      remaining_files: Arc::new(Mutex::new(VecDeque::new())),
      failures: Arc::new(Mutex::new(vec![])),
      ignore_failures,
      max_pool_size,
      max_download_attempts,
    }
  }

  // const MAXIMUM_POOL_SIZE: usize = 16;

  pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Utc::now();
    let mut futures = vec![];
    for _ in 0..self.max_pool_size {
      let job_name = self.name.clone();
      let remaining_files = Arc::clone(&self.remaining_files);
      let failures = Arc::clone(&self.failures);
      futures.push(
        tokio::spawn(async move {
          fn pop_downloadable(remaining_files: &Arc<Mutex<VecDeque<DownloadableSync>>>) -> Option<DownloadableSync> {
            let mut remaining_files = remaining_files.lock().unwrap();
            remaining_files.pop_front()
          }

          while let Some(downloadable) = pop_downloadable(&remaining_files) {
            if downloadable.get_start_time() == None {
              downloadable.set_start_time(Utc::now().timestamp_millis() as u64);
            }

            if downloadable.get_attempts() > (self.max_download_attempts as usize) {
              error!("Gave up trying to download {} for job '{}'", downloadable.url(), job_name);
              if !self.ignore_failures {
                failures.lock().unwrap().push(downloadable);
              }
            } else {
              info!(
                "Attempting to download {} for job '{}'... (try {})",
                downloadable.get_target_file().display(),
                job_name,
                downloadable.get_attempts()
              );

              let mut should_add_back = false;
              if let Err(err) = downloadable.download().await {
                warn!("Couldn't download {} for job '{}': {}", downloadable.url(), job_name, err);
                should_add_back = true;
              } else {
                info!("Finished downloading {} for job '{}'", downloadable.get_target_file().display(), job_name);
              }

              if should_add_back {
                remaining_files.lock().unwrap().push_back(downloadable);
              }
            }
          }
        })
      );
    }

    join_all(futures).await;
    let total_time = Utc::now().signed_duration_since(start_time).num_seconds();
    let failures = self.failures.lock().unwrap();
    if !failures.is_empty() {
      Err(MinecraftLauncherError(format!("Job '{}' finished with {} failure(s)! (took {}s)", self.name, failures.len(), total_time)))?;
    } else {
      info!("Job '{}' finished successfully (took {}s)", self.name, total_time);
    }
    Ok(())
  }

  pub fn add_downloadables(&self, downloadables: Vec<DownloadableSync>) {
    self.remaining_files.lock().unwrap().extend(downloadables);
  }
}
