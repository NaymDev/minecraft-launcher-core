use std::{ mem::take, sync::Arc, time::Duration };

use chrono::Utc;
use futures::{ stream::iter, StreamExt };
use log::{ error, info, warn };
use reqwest::{ header::{ HeaderMap, HeaderValue }, Client, Proxy };

use super::{ downloadables::{ DownloadError, Downloadable }, error::Error, progress::{ CallbackReporter, EmptyReporter, ProgressReporter } };

type DownloadableSync = Arc<dyn Downloadable + Send + Sync>;

pub struct DownloadJob {
  name: String,
  client: Client,
  all_files: Vec<Box<dyn Downloadable + Send + Sync>>,
  ignore_failures: bool,
  parallel_downloads: usize,
  retries: usize,

  // Tracks progress of the entire download job
  progress_reporter: ProgressReporter,
}

impl Default for DownloadJob {
  fn default() -> Self {
    Self {
      name: String::default(),

      client: Self::create_http_client(None).unwrap_or_default(),
      ignore_failures: false,
      parallel_downloads: 16,
      retries: 5,

      all_files: vec![],
      progress_reporter: Arc::new(EmptyReporter),
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

  pub fn with_parallel_downloads(mut self, concurrent_downloads: usize) -> Self {
    self.parallel_downloads = concurrent_downloads;
    self
  }

  pub fn with_retries(mut self, max_download_attempts: usize) -> Self {
    self.retries = max_download_attempts;
    self
  }

  pub fn with_progress_reporter(mut self, progress_reporter: &ProgressReporter) -> Self {
    self.progress_reporter = Arc::clone(progress_reporter);
    self
  }

  pub fn add_downloadables(mut self, mut downloadables: Vec<Box<dyn Downloadable + Send + Sync>>) -> Self {
    self.all_files.append(&mut downloadables);
    self
  }

  fn prepare_downloadables(&mut self) -> Vec<DownloadableSync> {
    let all_files: Vec<DownloadableSync> = take(&mut self.all_files).into_iter().map(Arc::from).collect();
    let monitor = JobMonitor::new(self.progress_reporter.clone(), &all_files);

    all_files.iter().for_each(|downloadable| {
      downloadable.get_monitor().set_reporter(monitor.download_reporter());
    });

    all_files
  }
}

impl DownloadJob {
  pub async fn start(mut self) -> Result<(), Error> {
    // self.progress_reporter.clear();

    let start_time = Utc::now();
    let downloadables = self.prepare_downloadables();

    self.progress_reporter.setup(&format!("Starting \"{}\"", self.name), None);
    let results = self.run(downloadables).await;

    let total_time = Utc::now().signed_duration_since(start_time).num_seconds();
    let failures = results
      .iter()
      .filter(|r| r.is_err())
      .collect::<Vec<_>>();

    self.progress_reporter.done();

    if self.ignore_failures || failures.is_empty() {
      info!("Job '{}' finished successfully (took {}s)", self.name, total_time);
      return Ok(());
    }
    Err(Error::JobFailed { name: self.name, failures: failures.len(), total_time })
  }

  async fn run(&self, downloads: Vec<DownloadableSync>) -> Vec<Result<DownloadableSync, DownloadError>> {
    let job_name = self.name.clone();
    let client = self.client.clone();
    let retries = self.retries;
    let parallel_downloads = self.parallel_downloads;

    let iter = iter(downloads)
      .map(move |downloadable| (downloadable, job_name.clone(), client.clone(), retries))
      .map(|(downloadable, job_name, client, retries)| download(job_name, client, retries, downloadable))
      .buffer_unordered(parallel_downloads);

    // FIXME: currently, this was the only way i've found to make the future returned by the function implement `Send`
    tokio::spawn(iter.collect()).await.unwrap()
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

async fn download(job_name: String, client: Client, retries: usize, downloadable: DownloadableSync) -> Result<DownloadableSync, DownloadError> {
  if downloadable.get_start_time().is_none() {
    downloadable.set_start_time(Utc::now().timestamp_millis() as u64);
  }

  let target_file = downloadable.get_target_file();

  let mut last_error = None;
  for attempt in 0..retries {
    info!("Attempting to download {} for job '{}'... (try {})", target_file.display(), job_name, attempt);

    let download_result = downloadable.download(&client).await;

    let monitor = downloadable.get_monitor();
    monitor.set_current(monitor.get_total());

    match download_result {
      Ok(_) => {
        info!("Finished downloading {} for job '{}'", target_file.display(), job_name);
        downloadable.set_end_time(Utc::now().timestamp_millis() as u64);
        return Ok(downloadable);
      }
      Err(err) => {
        warn!("Couldn't download {} for job '{}': {}", downloadable.url(), job_name, err);
        last_error.replace(err);
      }
    }
  }

  error!("Gave up trying to download {} for job '{}'", downloadable.url(), job_name);
  match last_error {
    Some(err) => Err(err),
    None => Ok(downloadable),
  }
}

#[derive(Clone)]
pub struct JobMonitor {
  reporter: ProgressReporter,
  files: Vec<DownloadableSync>,
}

impl JobMonitor {
  pub fn new(reporter: ProgressReporter, files: &[DownloadableSync]) -> Self {
    Self { reporter, files: files.to_vec() }
  }

  pub fn fire_update(&self) {
    let mut current_size = 0;
    let mut total_size = 0;

    let mut displayed_file: Option<&DownloadableSync> = None;

    for file in &self.files {
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
      self.reporter.status(&displayed_file.get_status());
      self.reporter.total(total_size);
      self.reporter.progress(current_size);
    } else {
      self.reporter.done();
    }
  }

  pub fn download_reporter(&self) -> ProgressReporter {
    let monitor = self.clone();
    Arc::new(CallbackReporter::new(move |_| monitor.fire_update()))
  }
}
