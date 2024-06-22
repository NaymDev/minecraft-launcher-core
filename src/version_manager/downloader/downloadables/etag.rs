use std::{ ffi::OsStr, fs, path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use log::info;
use reqwest::{ header::HeaderValue, Client };

use crate::version_manager::downloader::error::Error;

use super::{ Downloadable, DownloadableMonitor };

pub struct EtagDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub monitor: Arc<DownloadableMonitor>,
}

impl EtagDownloadable {
  pub fn new(url: &str, target_file: &Path, force_download: bool) -> Self {
    Self {
      url: url.to_string(),
      target_file: target_file.to_path_buf(),
      force_download,
      attempts: Arc::new(Mutex::new(0)),
      start_time: Arc::new(Mutex::new(None)),
      end_time: Arc::new(Mutex::new(None)),

      monitor: Arc::new(DownloadableMonitor::new(0, 5242880)),
    }
  }

  fn get_etag(etag: Option<&HeaderValue>) -> String {
    let etag = etag.and_then(|v| v.to_str().ok()).unwrap_or("-");
    if etag.starts_with('"') && etag.ends_with('"') {
      return etag[1..etag.len() - 1].to_string();
    }
    etag.to_string()
  }
}

#[async_trait]
impl Downloadable for EtagDownloadable {
  fn url(&self) -> &String {
    &self.url
  }

  fn get_target_file(&self) -> &PathBuf {
    &self.target_file
  }

  fn force_download(&self) -> bool {
    self.force_download
  }

  fn get_attempts(&self) -> usize {
    *self.attempts.lock().unwrap()
  }

  fn get_status(&self) -> String {
    let file_name = self.get_target_file().file_name().and_then(OsStr::to_str).unwrap_or(self.url());
    format!("Downloading {}", file_name)
  }

  fn get_monitor(&self) -> &Arc<DownloadableMonitor> {
    &self.monitor
  }

  fn get_start_time(&self) -> Option<u64> {
    *self.start_time.lock().unwrap()
  }

  fn set_start_time(&self, start_time: u64) {
    *self.start_time.lock().unwrap() = Some(start_time);
  }

  fn get_end_time(&self) -> Option<u64> {
    *self.end_time.lock().unwrap()
  }

  fn set_end_time(&self, end_time: u64) {
    *self.end_time.lock().unwrap() = Some(end_time);
  }

  async fn download(&self, client: &Client) -> Result<(), Error> {
    if let Ok(mut attempts) = self.attempts.lock() {
      *attempts += 1;
    }
    self.ensure_file_writable(&self.target_file)?;

    let target = &self.target_file;
    let res = client.get(&self.url).send().await?.error_for_status()?;
    if let Some(content_len) = res.content_length() {
      self.monitor.set_total(content_len as usize);
    }
    let etag = Self::get_etag(res.headers().get("ETag"));
    let bytes = res.bytes().await?;
    let md5 = md5::compute(&bytes).0;
    fs::write(target, &bytes)?;
    if etag.contains('-') {
      info!("Didn't have etag so assuming our copy is good");
      return Ok(());
    } else if etag.as_bytes() == md5 {
      info!("Downloaded successfully and etag matched");
      return Ok(());
    } else {
      return Err(Error::Other(format!("Couldn't connect to server (responded with {}), but have local file, assuming it's good", etag)));
    }
  }
}
