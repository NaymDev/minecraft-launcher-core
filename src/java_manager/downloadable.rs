use std::{ fs::{ self, File }, io::{ BufReader, Write }, path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use log::{ info, warn };
use lzma_rs::lzma_decompress;
use md5::Digest;
use reqwest::Client;
use sha1::Sha1;

use crate::{
  java_manager::{ error::CompressedRuntimeFileError, manifest::Download },
  json::Sha1Sum,
  version_manager::downloader::downloadables::{ DownloadError, Downloadable, DownloadableMonitor, HashError },
};

use super::manifest::Downloads;

pub struct RuntimeFileDownloadable {
  pub file_name: String,
  pub object_loc: Option<PathBuf>,
  pub target_file: PathBuf,
  pub downloads: Downloads,
  pub executable: bool,

  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,
  pub status: Mutex<RuntimeFileDownloadableStatus>,
  pub monitor: Arc<DownloadableMonitor>,
}

impl RuntimeFileDownloadable {
  pub fn new(name: &str, runtime_downloads: Downloads, executable: bool, objects_dir: &Path, target_file: &Path) -> Self {
    Self {
      file_name: name.to_string(),
      object_loc: if let Some(lzma) = &runtime_downloads.lzma {
        let sha1 = lzma.sha1.to_string();
        Some(objects_dir.join(&sha1[0..2]).join(sha1))
      } else {
        None
      },
      target_file: target_file.to_path_buf(),
      downloads: runtime_downloads,
      executable,

      start_time: Default::default(),
      end_time: Default::default(),
      status: Default::default(),
      monitor: Arc::new(DownloadableMonitor::new(0, 5242880)),
    }
  }

  fn unpack_lzma(&self, file: &Path) -> Result<Sha1Sum, DownloadError> {
    let target = self.get_target_file();
    self.set_status(RuntimeFileDownloadableStatus::Decompressing);

    self.prepare_destination(target).map_err(DownloadError::PrepareDestination)?;

    let compressed_file = File::open(file).map_err(CompressedRuntimeFileError::ReadLzma)?;
    let mut reader = BufReader::new(compressed_file);

    let mut bytes = vec![];
    lzma_decompress(&mut reader, &mut bytes).map_err(CompressedRuntimeFileError::Decompress)?;

    fs::write(target, &bytes).map_err(DownloadError::WriteFile)?;
    let _ = set_executable(target);

    let mut sha1 = Sha1::new();
    sha1.update(bytes);
    Ok(Sha1Sum::from(sha1))
  }

  async fn try_download_lzma(
    &self,
    client: &Client,
    compressed_url: &str,
    compressed_target: &PathBuf
  ) -> Result<Sha1Sum, CompressedRuntimeFileError> {
    let monitor = self.get_monitor();

    self.prepare_destination(compressed_target).map_err(CompressedRuntimeFileError::PrepareDestination)?;
    let mut response = client.get(compressed_url).send().await?.error_for_status()?;
    if let Some(content_len) = response.content_length() {
      monitor.set_total(content_len as usize);
    }

    let mut sha1 = Sha1::new();
    let mut file = File::create(compressed_target).map_err(CompressedRuntimeFileError::WriteLzma)?;
    while let Some(bytes) = response.chunk().await? {
      file.write_all(&bytes).map_err(CompressedRuntimeFileError::WriteLzma)?;
      file.flush().map_err(CompressedRuntimeFileError::WriteLzma)?;
      sha1.update(&bytes);
    }
    Ok(Sha1Sum::from(sha1))
  }

  fn set_status(&self, status: RuntimeFileDownloadableStatus) {
    *self.status.lock().unwrap() = status;
  }
}

#[async_trait]
impl Downloadable for RuntimeFileDownloadable {
  fn url(&self) -> &String {
    &self.downloads.raw.url
  }

  fn get_target_file(&self) -> &PathBuf {
    &self.target_file
  }

  fn get_status(&self) -> String {
    let status = self.status.lock().unwrap();
    format!("{} {}", status.as_str(), self.file_name)
  }

  fn get_monitor(&self) -> &Arc<DownloadableMonitor> {
    &self.monitor
  }

  fn get_start_time(&self) -> Option<u64> {
    *self.start_time.lock().unwrap()
  }

  fn set_start_time(&self, start_time: u64) {
    self.start_time.lock().unwrap().replace(start_time);
  }

  fn get_end_time(&self) -> Option<u64> {
    *self.end_time.lock().unwrap()
  }

  fn set_end_time(&self, end_time: u64) {
    self.end_time.lock().unwrap().replace(end_time);
  }

  async fn get_expected_hash(&self, _: &Client) -> Result<Option<Vec<u8>>, HashError> {
    let sha1 = self.downloads.raw.sha1.as_slice();
    Ok(Some(sha1.to_vec()))
  }

  async fn download(&self, client: &Client) -> Result<(), DownloadError> {
    let target = self.get_target_file();
    let expected_hash = self.get_expected_hash(client).await?;

    // 1. Check if the file is already downloaded
    if target.is_file() {
      if let Some(expected) = &expected_hash {
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

    // 2. - If the file has a compressed version, download (if needed) and unpack
    //    - If the file has no compressed version, download normally
    self.set_status(RuntimeFileDownloadableStatus::Downloading);
    let actual_hash = match &self.downloads.lzma {
      None => {
        let hash = self.try_download(client).await;
        let _ = set_executable(&self.target_file);
        hash
      }
      Some(Download { url, sha1, .. }) => {
        let object_loc = self.object_loc.as_ref().unwrap();

        if object_loc.is_file() {
          let mut compressed_file = File::open(object_loc).map_err(CompressedRuntimeFileError::ReadLzma)?;
          let actual_compressed_hash = Sha1Sum::from_reader(&mut compressed_file).map_err(CompressedRuntimeFileError::ReadLzma)?;
          if &actual_compressed_hash != sha1 {
            warn!("Had local compressed but it was the wrong hash... expected {} but had {}", sha1, actual_compressed_hash);
            fs::remove_file(object_loc).map_err(CompressedRuntimeFileError::RemoveLzma)?;
          }
        }

        if !object_loc.is_file() {
          let actual_compressed_hash = self.try_download_lzma(client, url, object_loc).await?;
          // If compressed hash matches
          if &actual_compressed_hash != sha1 {
            let _ = fs::remove_file(object_loc);
            return Err(DownloadError::ChecksumMismatch { expected: sha1.clone().into(), actual: actual_compressed_hash.into() });
          }
        }
        self.unpack_lzma(object_loc)
      }
    };
    let actual_hash = actual_hash?;

    // 3. Verify download
    if let Some(expected) = &expected_hash {
      if actual_hash.as_slice() != expected {
        // Try to remove. If it fails, we don't care
        let _ = fs::remove_file(target);
        return Err(DownloadError::ChecksumMismatch { expected: expected.clone(), actual: actual_hash.into() });
      }

      info!("Downloaded file and hash matched successfully");
    } else {
      info!("Successfully downloaded file but no checksum provided, assuming it's good");
    }

    info!("Downloaded successfully and checksum matched");
    Ok(())
  }
}

#[cfg(target_os = "linux")]
fn set_executable(target: &Path) -> Result<(), std::io::Error> {
  if let Ok(metadata) = target.metadata() {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(target, permissions)?;
  }
  Ok(())
}

#[cfg(not(target_os = "linux"))]
fn set_executable(_: &Path) -> Result<(), std::io::Error> {
  Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum RuntimeFileDownloadableStatus {
  #[default] Downloading,
  Decompressing,
}

impl RuntimeFileDownloadableStatus {
  pub fn as_str(&self) -> &str {
    match self {
      Self::Downloading => "Downloading",
      Self::Decompressing => "Decompressing",
    }
  }
}
