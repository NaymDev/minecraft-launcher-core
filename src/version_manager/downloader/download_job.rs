use std::{ mem::take, sync::Arc, time::Duration };

use chrono::Utc;
use futures::{ stream::iter, StreamExt };
use log::{ error, info, warn };
use reqwest::{ header::{ HeaderMap, HeaderValue }, Client, Proxy };

use super::{ downloadables::Downloadable, error::Error, progress_reporter::ProgressReporter };

type DownloadableSync = Arc<dyn Downloadable + Send + Sync>;

pub struct DownloadJob {
  name: String,
  client: Client,
  all_files: Vec<Box<dyn Downloadable + Send + Sync>>,
  ignore_failures: bool,
  concurrent_downloads: u16,
  max_download_attempts: u8,

  // Tracks progress of the entire download job
  progress_reporter: Arc<ProgressReporter>,
}

impl Default for DownloadJob {
  fn default() -> Self {
    Self {
      name: String::default(),

      client: Self::create_http_client(None).unwrap_or_default(),
      ignore_failures: false,
      concurrent_downloads: 16,
      max_download_attempts: 5,

      all_files: vec![],
      progress_reporter: Arc::default(),
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
    self
  }

  pub fn add_downloadables(mut self, mut downloadables: Vec<Box<dyn Downloadable + Send + Sync>>) -> Self {
    self.all_files.append(&mut downloadables);
    self
  }

  fn prepare_downloadables(&mut self) -> Vec<DownloadableSync> {
    let all_files: Vec<DownloadableSync> = take(&mut self.all_files).into_iter().map(Arc::from).collect();

    let reporter = {
      let progress_reporter = Arc::clone(&self.progress_reporter);
      let all_files = all_files.clone();
      Arc::new(
        ProgressReporter::new(move |_update| {
          Self::update_progress(&all_files, &progress_reporter);
        })
      )
    };

    for downloadable in all_files.iter() {
      downloadable.get_monitor().set_reporter(Arc::clone(&reporter));
    }

    all_files
  }
}

impl DownloadJob {
  pub async fn start(mut self) -> Result<(), Error> {
    self.progress_reporter.clear();

    let start_time = Utc::now();
    let downloadables = self.prepare_downloadables();
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

    let target_file = downloadable.get_target_file();
    loop {
      info!("Attempting to download {} for job '{}'... (try {})", target_file.display(), self.name, downloadable.get_attempts());
      let download_result = downloadable.download(&self.client).await;

      let monitor = downloadable.get_monitor();
      monitor.set_current(monitor.get_total());

      match download_result {
        Ok(_) => {
          info!("Finished downloading {} for job '{}'", target_file.display(), self.name);
          downloadable.set_end_time(Utc::now().timestamp_millis() as u64);
          break Ok(downloadable);
        }
        Err(err) => {
          warn!("Couldn't download {} for job '{}': {}", downloadable.url(), self.name, err);
          if downloadable.get_attempts() > (self.max_download_attempts as usize) {
            error!("Gave up trying to download {} for job '{}'", downloadable.url(), self.name);
            break Err(err);
          }
        }
      }
    }
  }

  fn update_progress(all_files: &Vec<DownloadableSync>, progress_reporter: &ProgressReporter) {
    let mut current_size = 0;
    let mut total_size = 0;

    let mut displayed_file: Option<&DownloadableSync> = None;

    for file in all_files {
      current_size += file.get_monitor().get_current();
      total_size += file.get_monitor().get_total();

      if file.get_end_time().is_none() {
        // If `file` started first, or if `displayed` has finished during the loop, replace it
        if let Some(displayed) = displayed_file {
          if file.get_start_time() >= displayed.get_start_time() && displayed.get_end_time().is_none() {
            continue;
          }
        }
        displayed_file.replace(file);
      }
    }

    if let Some(displayed_file) = displayed_file {
      let status = displayed_file.get_status();
      let scaled_current = (((current_size as f64) / (total_size as f64)) * 100.0).ceil();
      progress_reporter.set(status, scaled_current as u32, 100);
    } else {
      progress_reporter.clear();
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
