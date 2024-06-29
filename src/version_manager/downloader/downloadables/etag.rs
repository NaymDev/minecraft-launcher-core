use std::{ ffi::OsStr, fs::File, io, path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use log::info;
use md5::{ Digest, Md5 };
use reqwest::{ header::HeaderValue, Client };

use super::{ error::HashError, DownloadError, Downloadable, DownloadableMonitor };

pub struct EtagDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub force_download: bool,
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

  async fn get_expected_hash(&self, client: &Client) -> Result<Option<Vec<u8>>, HashError> {
    let response = client.get(self.url()).send().await?.error_for_status()?;
    let hash = Self::get_etag(response.headers().get("ETag"));
    if hash.contains('-') {
      Ok(None)
    } else {
      Ok(Some(hex::decode(hash)?))
    }
  }

  fn calculate_local_hash(&self) -> Result<Vec<u8>, HashError> {
    let target_file = self.get_target_file();
    let mut reader = File::open(target_file)?;
    let mut md5 = Md5::new();
    io::copy(&mut reader, &mut md5)?;
    Ok(md5.finalize().to_vec())
  }

  async fn download(&self, client: &Client) -> Result<(), DownloadError> {
    let target = &self.target_file;

    if target.is_file() && !self.force_download {
      info!("Local file exists, assuming it's good");
      return Ok(());
    }

    self.try_download(client).await?;
    let expected = self.get_expected_hash(client).await?;

    if let Some(etag) = expected {
      let md5 = self.calculate_local_hash()?;
      if etag != md5 {
        return Err(DownloadError::ChecksumMismatch { expected: etag, actual: md5 });
      }
      info!("Downloaded successfully and etag matched");
    } else {
      info!("Didn't have etag so assuming our copy is good");
    }
    Ok(())
  }
}
