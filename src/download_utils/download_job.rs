use std::{ sync::{ Arc, Mutex, RwLock }, collections::VecDeque };

use chrono::Utc;
use futures::future::join_all;
use log::{ info, error, warn };

use crate::{ bootstrap::MinecraftLauncherError, progress_reporter::ProgressReporter };

use super::Downloadable;

type DownloadableSync = Arc<dyn Downloadable + Send + Sync>;

pub struct DownloadJob {
  name: String,
  all_files: Arc<RwLock<Vec<DownloadableSync>>>,
  remaining_files: Arc<Mutex<VecDeque<DownloadableSync>>>,
  failures: Arc<Mutex<Vec<DownloadableSync>>>,
  ignore_failures: bool,
  max_pool_size: u16,
  max_download_attempts: u8,

  progress_reporter: Arc<ProgressReporter>,
  downloadable_progress_reporter: Arc<ProgressReporter>,
}

impl DownloadJob {
  pub fn new(name: &str, ignore_failures: bool, max_pool_size: u16, max_download_attempts: u8, progress_reporter: &Arc<ProgressReporter>) -> Self {
    let progress_reporter = Arc::clone(progress_reporter);
    let all_files = Arc::new(RwLock::new(vec![]));
    let downloadable_progress_reporter = {
      let progress_reporter = Arc::clone(&progress_reporter);
      let all_files = Arc::clone(&all_files);
      Arc::new(
        ProgressReporter::new(move |_update| {
          Self::update_progress(&all_files, &progress_reporter);
        })
      )
    };

    Self {
      name: name.to_string(),
      all_files,
      remaining_files: Arc::new(Mutex::new(VecDeque::new())),
      failures: Arc::new(Mutex::new(vec![])),
      ignore_failures,
      max_pool_size,
      max_download_attempts,
      progress_reporter,
      downloadable_progress_reporter,
    }
  }

  // const MAXIMUM_POOL_SIZE: usize = 16;

  pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
    self.progress_reporter.clear();

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
                downloadable.set_end_time(Utc::now().timestamp_millis() as u64);
              }

              let monitor = downloadable.get_monitor();
              monitor.set_current(monitor.get_total());

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

    self.progress_reporter.clear();
    Ok(())
  }

  pub fn add_downloadables(&mut self, downloadables: Vec<Box<dyn Downloadable + Send + Sync>>) {
    let mut all_files = self.all_files.write().unwrap();
    let mut remaining_files = self.remaining_files.lock().unwrap();
    for downloadable in downloadables {
      downloadable.get_monitor().set_reporter(self.downloadable_progress_reporter.clone());
      let downloadable_arc = Arc::from(downloadable);
      remaining_files.push_back(Arc::clone(&downloadable_arc));
      all_files.push(downloadable_arc);
    }
  }

  fn update_progress(all_files: &RwLock<Vec<DownloadableSync>>, progress_reporter: &ProgressReporter) {
    if let Ok(all_files) = all_files.try_read() {
      let all_files = &*all_files;
      if all_files.is_empty() {
        progress_reporter.clear();
        return;
      }

      let mut current_size = 0;
      let mut total_size = 0;
      let mut last_file: Option<&DownloadableSync> = None;
      for file in all_files {
        current_size += file.get_monitor().get_current();
        total_size += file.get_monitor().get_total();

        if let Some(last_file) = last_file {
          if last_file.get_end_time().is_none() && (file.get_start_time() >= last_file.get_start_time() || file.get_end_time().is_some()) {
            continue;
          }
        }
        last_file = Some(file);
      }

      let status = last_file.map(|file| file.get_status()).unwrap_or_default();
      let scaled_current = (((current_size as f64) / (total_size as f64)) * 100.0).ceil();
      progress_reporter.set(status, scaled_current as u32, 100);
    }
  }
}
