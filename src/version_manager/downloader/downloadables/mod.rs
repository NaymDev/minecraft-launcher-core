use std::{ fs::{ self, create_dir_all, File }, io::Write, path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use log::info;
use reqwest::Client;
use sha1::{ Digest, Sha1 };

use crate::json::Sha1Sum;

mod checksummed;
mod prehashed;
mod etag;
mod asset;

mod error;

pub use error::{ DownloadError, HashError };

pub use checksummed::ChecksummedDownloadable;
pub use prehashed::PreHashedDownloadable;
pub use etag::EtagDownloadable;
pub use asset::{ AssetDownloadable, AssetDownloadableStatus };

use super::progress::{ EmptyReporter, ProgressReporter };

#[async_trait]
pub trait Downloadable: Send + Sync {
  fn url(&self) -> &String;
  fn get_target_file(&self) -> &PathBuf;
  fn get_status(&self) -> String;
  fn get_monitor(&self) -> &Arc<DownloadableMonitor>;

  fn get_start_time(&self) -> Option<u64>;
  fn set_start_time(&self, start_time: u64);
  fn get_end_time(&self) -> Option<u64>;
  fn set_end_time(&self, end_time: u64);

  // TODO: remove?
  fn prepare_destination(&self, file: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = file.parent() {
      if !parent.is_dir() {
        info!("Making directory {}", parent.display());
        create_dir_all(parent)?;
      }
    }

    Ok(())
  }

  async fn get_expected_hash(&self, client: &Client) -> Result<Option<Vec<u8>>, HashError>;

  fn calculate_local_hash(&self) -> Result<Vec<u8>, HashError> {
    let target_file = self.get_target_file();
    let mut reader = File::open(target_file)?;
    Ok(Sha1Sum::from_reader(&mut reader)?.into())
  }

  // Forces the download even if the target file already exists
  async fn try_download(&self, client: &Client) -> Result<Sha1Sum, DownloadError> {
    let target_file = self.get_target_file();
    let monitor = self.get_monitor();

    self.prepare_destination(target_file).map_err(DownloadError::PrepareDestination)?;

    let mut response = client.get(self.url()).send().await?.error_for_status()?;
    if let Some(content_len) = response.content_length() {
      monitor.set_total(content_len as usize);
    }

    let mut file = File::create(target_file).map_err(DownloadError::WriteFile)?;
    let mut sha1 = Sha1::new();

    while let Some(chunk) = response.chunk().await? {
      file.write_all(&chunk).map_err(DownloadError::WriteFile)?;
      file.flush().map_err(DownloadError::WriteFile)?;
      sha1.update(&chunk);
    }

    Ok(sha1.into())
  }

  // async fn download(&self, client: &Client) -> Result<(), DownloadError>;
  async fn download(&self, client: &Client) -> Result<(), DownloadError> {
    let target = self.get_target_file();
    let expected = self.get_expected_hash(client).await?;

    if target.is_file() {
      if let Some(expected) = &expected {
        let actual = self.calculate_local_hash()?;
        if actual == *expected {
          info!("Local file matches hash, using it");
          return Ok(());
        }
        // Hash mismatch, remove the file
        fs::remove_file(target).map_err(DownloadError::RemoveFile)?;
      } else {
        info!("Local file exists, but no hash, assuming it's good");
        return Ok(());
      }
    }

    let actual = self.try_download(client).await?;
    if let Some(expected) = &expected {
      if actual.as_slice() != expected {
        // Try to remove. If it fails, we don't care
        let _ = fs::remove_file(target);
        return Err(DownloadError::ChecksumMismatch { expected: expected.clone(), actual: actual.into() });
      }

      info!("Downloaded successfully and checksum matched");
    } else {
      info!("Downloaded successfully but no checksum provided, assuming it's good");
    }
    Ok(())
  }
}

pub struct DownloadableMonitor {
  current: Mutex<usize>,
  total: Mutex<usize>,
  reporter: Mutex<ProgressReporter>,
}

impl DownloadableMonitor {
  pub fn new(current: usize, total: usize) -> Self {
    Self {
      current: Mutex::new(current),
      total: Mutex::new(total),
      reporter: Mutex::new(Arc::new(EmptyReporter)),
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
    self.reporter.lock().unwrap().progress(current);
  }

  pub fn set_total(&self, total: usize) {
    *self.total.lock().unwrap() = total;
    self.reporter.lock().unwrap().total(total);
  }

  pub fn set_reporter(&self, reporter: ProgressReporter) {
    *self.reporter.lock().unwrap() = reporter;
    // TODO: fire update?
  }
}
