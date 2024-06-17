pub mod download_job;

use std::{ sync::{ Arc, Mutex }, time::Duration };
use reqwest::{ header::{ HeaderMap, HeaderValue }, Client, Proxy };
use crate::progress_reporter::ProgressReporter;

pub mod downloadables;

#[derive(Debug, Clone, Default)]
pub enum ProxyOptions {
  #[default] NoProxy,
  Proxy(reqwest::Url),
}

impl ProxyOptions {
  fn client_builder(&self) -> reqwest::ClientBuilder {
    let mut builder = Client::builder();
    if let ProxyOptions::Proxy(url) = self {
      builder = builder.proxy(Proxy::all(url.as_str()).unwrap());
    }
    builder
  }

  pub fn create_http_client(&self) -> Client {
    let mut headers = HeaderMap::new();
    headers.append("Cache-Control", HeaderValue::from_static("no-store,max-age=0,no-cache"));
    headers.append("Expires", HeaderValue::from_static("0"));
    headers.append("Pragma", HeaderValue::from_static("no-cache"));

    let builder = self.client_builder().default_headers(headers).timeout(Duration::from_secs(15));
    builder.build().unwrap_or(Client::new())
  }
}

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
