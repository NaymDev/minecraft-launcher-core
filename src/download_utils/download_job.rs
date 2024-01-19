use std::{ sync::{ Arc, Mutex }, collections::VecDeque };

use chrono::Utc;
use futures::future::join_all;
use log::{ info, error, warn, debug };

use crate::{ MinecraftLauncherError, monitor::{ Monitor, DownloadProgress } };

use super::Downloadable;

type DownloadableSync = Box<dyn Downloadable + Send + Sync>;

pub struct DownloadJob {
  name: String,
  started: bool,
  all_files: Arc<Mutex<Vec<DownloadableSync>>>,
  remaining_files: Arc<Mutex<VecDeque<DownloadableSync>>>,
  failures: Arc<Mutex<Vec<DownloadableSync>>>,
  ignore_failures: bool,
  max_pool_size: u16,
  max_download_attempts: u8,
  monitor: Arc<Mutex<dyn Monitor + Send + Sync>>,
}

impl DownloadJob {
  pub fn new(
    name: &str,
    ignore_failures: bool,
    max_pool_size: u16,
    max_download_attempts: u8,
    monitor: Arc<Mutex<dyn Monitor + Send + Sync>>
  ) -> Self {
    Self {
      name: name.to_string(),
      started: false,
      all_files: Arc::new(Mutex::new(vec![])),
      remaining_files: Arc::new(Mutex::new(VecDeque::new())),
      failures: Arc::new(Mutex::new(vec![])),
      ignore_failures,
      max_pool_size,
      max_download_attempts,
      monitor,
    }
  }

  pub fn add_downloadables(&mut self, downloadables: Vec<DownloadableSync>) -> Result<(), MinecraftLauncherError> {
    if self.started {
      return Err(MinecraftLauncherError("Cannot add to download job that has already started!".to_string()).into());
    }

    let mut remaining_files = self.remaining_files.lock().unwrap();
    let mut all_files = self.all_files.lock().unwrap();
    for downloadable in &downloadables {
      all_files.push(downloadable.clone());
      remaining_files.push_back(downloadable.clone());
    }
    drop(remaining_files);
    drop(all_files);

    // Setup listener
    let monitor = Arc::downgrade(&self.monitor);
    let all_files = Arc::downgrade(&self.all_files);
    let on_update = move || {
      if let (Some(monitor), Some(all_files)) = (monitor.upgrade(), all_files.upgrade()) {
        let mut monitor = monitor.lock().unwrap();
        let all_files = &*all_files.lock().unwrap();
        if all_files.is_empty() {
          monitor.hide_download_progress();
          return;
        }

        let mut total_size = 0;
        let mut current_size = 0;
        let mut last_file: Option<&DownloadableSync> = None;
        for file in all_files {
          // Avoid possible deadlocks
          if let Ok(file_monitor) = file.get_monitor().try_lock() {
            total_size += file_monitor.get_total();
            current_size += file_monitor.get_current();

            if let Some(last) = last_file {
              if last.get_end_time().is_none() && (file.get_start_time() >= last.get_start_time() || file.get_end_time().is_some()) {
                // last_file = Some(file);
                break;
              }
            }
            last_file = Some(file);
          }
        }
        monitor.set_download_progress(DownloadProgress::new(&last_file.unwrap().get_status(), current_size, total_size));
      }
    };

    for downloadable in downloadables {
      if let Ok(mut downloadable_monitor) = downloadable.get_monitor().lock() {
        downloadable_monitor.add_update_listener(on_update.clone());
        if let Some(size) = downloadable.get_expected_size() {
          downloadable_monitor.set_total(size);
        } else {
          downloadable_monitor.set_total(5242880);
        }
      }
    }
    on_update();
    Ok(())
  }

  pub async fn start(mut self) -> Result<(), Box<dyn std::error::Error>> {
    if self.started {
      return Err(MinecraftLauncherError("Cannot add to download job that has already started!".to_string()).into());
    }
    self.started = true;

    debug!("Ref checks: monitor({})", Arc::strong_count(&self.monitor));

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

              if let Ok(mut downloadable_monitor) = downloadable.get_monitor().lock() {
                let total = downloadable_monitor.get_total();
                downloadable_monitor.set_current(total);
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
      return Err(MinecraftLauncherError(format!("Job '{}' finished with {} failure(s)! (took {}s)", self.name, failures.len(), total_time)).into());
    }

    if let Ok(mut monitor) = self.monitor.lock() {
      monitor.hide_download_progress();
    }
    info!("Job '{}' finished successfully (took {}s)", self.name, total_time);
    Ok(())
  }
}
