use std::{ ffi::OsStr, fs::{ self, File }, io::Cursor, path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use log::info;
use reqwest::Client;

use crate::{ download_utils::{ error::Error, DownloadableMonitor }, json::Sha1Sum };

use super::Downloadable;

/// Both the file and the checksum are on the remote server
pub struct ChecksummedDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub monitor: Arc<DownloadableMonitor>,
}

impl ChecksummedDownloadable {
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

  async fn get_remote_hash(&self, client: &Client) -> Result<Sha1Sum, Box<dyn std::error::Error>> {
    let sha_url = format!("{}.sha1", self.url);
    let sum_hex = client.get(sha_url).send().await?.error_for_status()?.text().await?;
    Ok(Sha1Sum::try_from(sum_hex)?)
  }
}

#[async_trait]
impl Downloadable for ChecksummedDownloadable {
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

    let mut local_hash = None;
    let mut expected_hash = None;

    self.ensure_file_writable(&self.target_file)?;
    let target_file = self.get_target_file();

    // Try to get hash from local file
    if local_hash.is_none() && target_file.is_file() {
      local_hash = Some(Sha1Sum::from_reader(&mut File::open(target_file)?)?);
    }

    if expected_hash.is_none() {
      expected_hash = Some(self.get_remote_hash(client).await.unwrap_or(Sha1Sum::null()));
    }

    if expected_hash.as_ref().unwrap() == &Sha1Sum::null() && target_file.is_file() {
      info!("Couldn't find a checksum so assuming our copy is good");
      return Ok(());
    } else if expected_hash == local_hash {
      info!("Remote checksum matches local file");
      return Ok(());
    } else {
      let res = client.get(&self.url).send().await?.error_for_status()?;
      if let Some(content_len) = res.content_length() {
        self.monitor.set_total(content_len as usize);
      }
      let bytes = res.bytes().await?;
      local_hash = Some(Sha1Sum::from_reader(&mut Cursor::new(&bytes))?);
      fs::write(target_file, &bytes)?;
      if expected_hash.as_ref().unwrap() == &Sha1Sum::null() {
        info!("Didn't have checksum so assuming the downloaded file is good");
        return Ok(());
      } else if expected_hash == local_hash {
        info!("Downloaded successfully and checksum matched");
        return Ok(());
      } else {
        return Err(Error::ChecksumMismatch {
          expected: expected_hash.unwrap(),
          actual: local_hash.unwrap(),
        });
      }
    }
  }
}
