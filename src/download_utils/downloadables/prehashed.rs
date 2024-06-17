use std::{ ffi::OsStr, fs::{ self, File }, io::Cursor, path::PathBuf, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use log::info;
use reqwest::Client;

use crate::{ download_utils::DownloadableMonitor, json::Sha1Sum };

use super::Downloadable;

pub struct PreHashedDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub http_client: Client,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub expected_hash: Sha1Sum,
  pub monitor: Arc<DownloadableMonitor>,
}

impl PreHashedDownloadable {
  pub fn new(http_client: Client, url: &str, target_file: &PathBuf, force_download: bool, expected_hash: Sha1Sum) -> Self {
    Self {
      url: url.to_string(),
      target_file: target_file.to_path_buf(),
      http_client,
      force_download,
      attempts: Arc::new(Mutex::new(0)),
      start_time: Arc::new(Mutex::new(None)),
      end_time: Arc::new(Mutex::new(None)),

      expected_hash,
      monitor: Arc::new(DownloadableMonitor::new(0, 5242880)),
    }
  }
}

#[async_trait]
impl Downloadable for PreHashedDownloadable {
  fn get_http_client(&self) -> &Client {
    &self.http_client
  }

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
    self.start_time.lock().unwrap().clone()
  }

  fn set_start_time(&self, start_time: u64) {
    *self.start_time.lock().unwrap() = Some(start_time);
  }

  fn get_end_time(&self) -> Option<u64> {
    self.end_time.lock().unwrap().clone()
  }

  fn set_end_time(&self, end_time: u64) {
    *self.end_time.lock().unwrap() = Some(end_time);
  }

  async fn download(&self) -> Result<(), Box<dyn std::error::Error + 'life0>> {
    *self.attempts.lock()? += 1;
    self.ensure_file_writable(&self.target_file)?;
    let target = self.get_target_file();
    if target.is_file() {
      let local_hash = Sha1Sum::from_reader(&mut File::open(target)?)?;
      if local_hash == self.expected_hash {
        info!("Local file matches hash, using it");
        return Ok(());
      }
      fs::remove_file(target)?;
    }

    let res = self.make_connection(&self.url).await?;
    if let Some(content_len) = res.content_length() {
      self.monitor.set_total(content_len as usize);
    }
    let bytes = res.bytes().await?;
    let local_hash = Sha1Sum::from_reader(&mut Cursor::new(&bytes))?;
    fs::write(&target, &bytes)?;
    if local_hash == self.expected_hash {
      info!("Downloaded successfully and checksum matched");
      return Ok(());
    } else {
      Err(
        Box::new(
          std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Checksum did not match downloaded file (Checksum was {}, downloaded {})", self.expected_hash, local_hash)
          )
        )
      )?;
    }

    Ok(())
  }
}
