use std::{ sync::{ Arc, RwLock }, time::Duration };

use chrono::Utc;
use futures::{ stream::iter, StreamExt };
use log::{ info, error, warn };
use reqwest::{ header::{ HeaderMap, HeaderValue }, Client, Proxy };

use crate::progress_reporter::ProgressReporter;

use super::{ downloadables::Downloadable, error::Error };

type DownloadableSync = Arc<dyn Downloadable + Send + Sync>;

pub struct DownloadJob {
  name: String,
  client: Client,
  all_files: Arc<RwLock<Vec<DownloadableSync>>>,
  ignore_failures: bool,
  concurrent_downloads: u16,
  max_download_attempts: u8,

  // Tracks progress of the entire download job
  progress_reporter: Arc<ProgressReporter>,
  // Passed to each downloadable, to track progress of the entire download job
  downloadable_progress_reporter: Arc<ProgressReporter>,
}

impl Default for DownloadJob {
  fn default() -> Self {
    Self {
      name: String::default(),

      client: Self::create_http_client(None).unwrap_or_default(),
      ignore_failures: false,
      concurrent_downloads: 16,
      max_download_attempts: 5,

      all_files: Arc::default(),
      progress_reporter: Arc::default(),
      downloadable_progress_reporter: Arc::default(),
    }
  }
}

impl DownloadJob {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
      ..Self::default()
    }
  }

  pub fn with_client(mut self, client: Client) -> Self {
    self.client = client;
    self
  }

  pub fn ignore_failures(mut self, ignore_failures: bool) -> Self {
    self.ignore_failures = ignore_failures;
    self
  }

  pub fn concurrent_downloads(mut self, concurrent_downloads: u16) -> Self {
    self.concurrent_downloads = concurrent_downloads;
    self
  }

  pub fn max_download_attempts(mut self, max_download_attempts: u8) -> Self {
    self.max_download_attempts = max_download_attempts;
    self
  }

  pub fn with_progress_reporter(mut self, progress_reporter: &Arc<ProgressReporter>) -> Self {
    self.progress_reporter = Arc::clone(progress_reporter);

    let downloadable_progress_reporter = {
      let progress_reporter = Arc::clone(progress_reporter);
      let all_files = Arc::clone(&self.all_files);
      Arc::new(
        ProgressReporter::new(move |_update| {
          Self::update_progress(&all_files, &progress_reporter);
        })
      )
    };

    self.downloadable_progress_reporter = downloadable_progress_reporter;
    self
  }

  pub fn add_downloadables(self, downloadables: Vec<Box<dyn Downloadable + Send + Sync>>) -> Self {
    let mut all_files = self.all_files.write().unwrap();
    for downloadable in downloadables {
      downloadable.get_monitor().set_reporter(self.downloadable_progress_reporter.clone());
      let downloadable_arc = Arc::from(downloadable);
      all_files.push(downloadable_arc);
    }
    drop(all_files);
    self
  }
}

impl DownloadJob {
  pub async fn start(self) -> Result<(), Error> {
    self.progress_reporter.clear();

    let start_time = Utc::now();
    let downloadables = self.all_files.read().unwrap().to_vec();
    let results = iter(downloadables)
      .map(|downloadable| self.try_download(downloadable))
      .buffered(self.concurrent_downloads as usize)
      .collect::<Vec<_>>().await;

    let total_time = Utc::now().signed_duration_since(start_time).num_seconds();
    let failures = results
      .iter()
      .flat_map(|r| r.as_ref().err())
      .collect::<Vec<_>>();

    self.progress_reporter.clear();

    if self.ignore_failures || failures.is_empty() {
      info!("Job '{}' finished successfully (took {}s)", self.name, total_time);
      return Ok(());
    }
    Err(Error::JobFailed { name: self.name, failures: failures.len(), total_time })
  }

  async fn try_download(&self, downloadable: DownloadableSync) -> Result<DownloadableSync, Error> {
    if downloadable.get_start_time().is_none() {
      downloadable.set_start_time(Utc::now().timestamp_millis() as u64);
    }

    let mut download_result = Ok(&downloadable);
    let target_file = downloadable.get_target_file();
    while downloadable.get_attempts() <= (self.max_download_attempts as usize) {
      info!("Attempting to download {} for job '{}'... (try {})", target_file.display(), self.name, downloadable.get_attempts());
      download_result = downloadable.download(&self.client).await.map(|_| &downloadable);

      let monitor = downloadable.get_monitor();
      monitor.set_current(monitor.get_total());

      if let Err(err) = &download_result {
        warn!("Couldn't download {} for job '{}': {}", downloadable.url(), self.name, err);
      } else {
        info!("Finished downloading {} for job '{}'", target_file.display(), self.name);
        downloadable.set_end_time(Utc::now().timestamp_millis() as u64);
        break;
      }
    }

    if download_result.is_err() {
      error!("Gave up trying to download {} for job '{}'", downloadable.url(), self.name);
    }

    download_result.cloned()
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

impl DownloadJob {
  pub fn create_http_client(proxy: Option<Proxy>) -> Result<Client, reqwest::Error> {
    let mut client = Client::builder();
    let mut headers = HeaderMap::new();
    headers.append("Cache-Control", HeaderValue::from_static("no-store,max-age=0,no-cache"));
    headers.append("Expires", HeaderValue::from_static("0"));
    headers.append("Pragma", HeaderValue::from_static("no-cache"));

    client = client.default_headers(headers).connect_timeout(Duration::from_secs(30)).timeout(Duration::from_secs(15));
    if let Some(proxy) = proxy {
      client = client.proxy(proxy);
    }
    client.build()
  }
}
