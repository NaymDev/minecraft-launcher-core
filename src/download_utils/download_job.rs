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
}

impl DownloadJob {
  pub fn new(name: &str, ignore_failures: bool) -> Self {
    Self {
      name: name.to_string(),
      // all_files: vec![],
      remaining_files: Arc::new(Mutex::new(VecDeque::new())),
      failures: Arc::new(Mutex::new(vec![])),
      ignore_failures,
    }
  }

  // pub async fn start(self) {
  //   let start_time = Utc::now();
  //   let semaphore = Arc::new(Semaphore::new(self.max_concurrent_downloads));
  //   let failure_count = Arc::new(Mutex::new(0));

  //   if self.all_files.is_empty() {
  //     info!("Download job '{}' skipped as there are no files to download", self.name);
  //   } else {
  //     info!("Download job '{}' started ({} threads, {} files)", self.name, self.max_concurrent_downloads, self.all_files.len());
  //     let mut futures = vec![];
  //     for downloadable in self.all_files {
  //       let permit = semaphore.clone().acquire_owned().await.unwrap();
  //       let failure_count = failure_count.clone();
  //       futures.push(
  //         tokio::spawn(async move {
  //           if let Err(_) = downloadable.download(5).await {
  //             *failure_count.lock().unwrap() += 1;
  //             // if !self.ignore_failures {
  //             //   info!("Download job '{}' failed: {}", self.name, err);
  //             // }
  //             drop(permit);
  //           }
  //         })
  //       );
  //     }
  //     for future in futures {
  //       future.await.unwrap();
  //     }
  //     info!("Download job '{}' completed in {}ms", self.name, Utc::now().signed_duration_since(start_time).num_milliseconds());
  //     if !self.ignore_failures {
  //       info!("Download job '{}' failed to download {} files", self.name, *failure_count.lock().unwrap());
  //     }
  //   }
  // }

  // pub fn add_downloadables(&mut self, downloadables: Vec<Box<dyn Downloadable + Send + Sync>>) {
  //   self.all_files.extend(downloadables);
  // }

  const MAXIMUM_POOL_SIZE: usize = 16;

  pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Utc::now();
    let mut futures = vec![];
    for _ in 0..Self::MAXIMUM_POOL_SIZE {
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

            if downloadable.get_attempts() > 5 {
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
