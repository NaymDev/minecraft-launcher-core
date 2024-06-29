use std::{ fs::{ self, File }, io::{ Read, Write }, path::{ Path, PathBuf, MAIN_SEPARATOR_STR }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use libflate::non_blocking::gzip::Decoder as GzipDecoder;
use log::{ info, warn };
use reqwest::{ Client, Url };
use sha1::{ Digest, Sha1 };
use thiserror::Error;

use crate::json::{ manifest::assets::AssetObject, Sha1Sum };

use super::{ error::HashError, DownloadError, Downloadable, DownloadableMonitor };

pub struct AssetDownloadable {
  pub asset_name: String,
  pub asset_url: String,
  pub target_file: PathBuf,
  pub asset: AssetObject,

  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub status: Mutex<AssetDownloadableStatus>,
  pub url_base: Url,
  pub destination_dir: PathBuf,

  pub monitor: Arc<DownloadableMonitor>,
}

impl AssetDownloadable {
  pub fn new(asset_name: &str, asset: &AssetObject, url_base: &Url, objects_dir: &Path) -> Self {
    let path = AssetObject::create_path_from_hash(&asset.hash);
    let url = {
      let mut url = url_base.clone();
      url.set_path(&path);
      url.to_string()
    };

    let target_file = objects_dir.join(path.replace('/', MAIN_SEPARATOR_STR));
    Self {
      asset_url: url,
      target_file,
      start_time: Arc::new(Mutex::new(None)),
      end_time: Arc::new(Mutex::new(None)),

      asset_name: asset_name.to_string(),
      status: Mutex::new(AssetDownloadableStatus::Downloading),
      asset: asset.clone(),
      url_base: url_base.clone(),
      destination_dir: objects_dir.to_path_buf(),
      monitor: Arc::new(DownloadableMonitor::new(0, asset.size)),
    }
  }

  fn decompress_asset(&self, compressed_target: &PathBuf) -> Result<Sha1Sum, DownloadError> {
    let target = self.get_target_file();
    self.set_status(AssetDownloadableStatus::Extracting);

    self.prepare_destination(target).map_err(DownloadError::PrepareDestination)?;

    let mut compressed_file = File::open(compressed_target).map_err(CompressedAssetError::ReadAsset)?;
    let mut decoder = GzipDecoder::new(&mut compressed_file);

    let mut sha1 = Sha1::new();
    let mut file = File::create(target).map_err(DownloadError::WriteFile)?;

    let mut read_buf = [0; 8192];
    while let Ok(size) = decoder.read(&mut read_buf) {
      if size == 0 {
        break;
      }
      let buf = &read_buf[..size];
      file.write_all(buf).map_err(DownloadError::WriteFile)?;
      file.flush().map_err(DownloadError::WriteFile)?;
      sha1.update(buf);
    }
    Ok(Sha1Sum::from(sha1))
  }

  async fn try_download_compressed(
    &self,
    client: &Client,
    compressed_url: &str,
    compressed_target: &PathBuf
  ) -> Result<Sha1Sum, CompressedAssetError> {
    let monitor = self.get_monitor();

    self.prepare_destination(compressed_target).map_err(CompressedAssetError::PrepareDestination)?;
    let mut response = client.get(compressed_url).send().await?.error_for_status()?;
    if let Some(content_len) = response.content_length() {
      monitor.set_total(content_len as usize);
    }

    let mut sha1 = Sha1::new();
    let mut file = File::create(compressed_target).map_err(CompressedAssetError::WriteFile)?;
    while let Some(bytes) = response.chunk().await? {
      file.write_all(&bytes).map_err(CompressedAssetError::WriteFile)?;
      file.flush().map_err(CompressedAssetError::WriteFile)?;
      sha1.update(&bytes);
    }
    Ok(Sha1Sum::from(sha1))
  }

  fn set_status(&self, status: AssetDownloadableStatus) {
    *self.status.lock().unwrap() = status;
  }
}

#[async_trait]
impl Downloadable for AssetDownloadable {
  fn url(&self) -> &String {
    &self.asset_url
  }

  fn get_target_file(&self) -> &PathBuf {
    &self.target_file
  }

  fn get_status(&self) -> String {
    format!("{} {}", self.status.lock().unwrap().as_str(), self.asset_name)
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

  async fn get_expected_hash(&self, _: &Client) -> Result<Option<Vec<u8>>, HashError> {
    let hash = self.asset.hash.clone();
    Ok(Some(hash.into()))
  }

  async fn download(&self, client: &Client) -> Result<(), DownloadError> {
    let target = self.get_target_file();
    let expected_hash = self.get_expected_hash(client).await?;

    // 1. Check if the asset is already downloaded
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

    // 2. - If the asset has a compressed version, download (if needed) and unpack
    //    - If the asset has no compressed version, download normally
    self.set_status(AssetDownloadableStatus::Downloading);
    let actual_hash = match &self.asset.compressed_hash {
      None => self.try_download(client).await,
      Some(expected_compressed_hash) => {
        let hash_path = AssetObject::create_path_from_hash(expected_compressed_hash);
        let compressed_target = self.destination_dir.join(&hash_path);

        let compressed_url = {
          let mut url = self.url_base.clone();
          url.set_path(&hash_path);
          url.to_string()
        };

        if compressed_target.is_file() {
          let mut compressed_file = File::open(&compressed_target).map_err(CompressedAssetError::ReadAsset)?;
          let actual_compressed_hash = Sha1Sum::from_reader(&mut compressed_file).map_err(CompressedAssetError::ReadAsset)?;
          if &actual_compressed_hash != expected_compressed_hash {
            warn!("Had local compressed but it was the wrong hash... expected {} but had {}", expected_compressed_hash, actual_compressed_hash);
            fs::remove_file(&compressed_target).map_err(CompressedAssetError::RemoveFile)?;
          }
        }

        if !compressed_target.is_file() {
          let actual_compressed_hash = self.try_download_compressed(client, &compressed_url, &compressed_target).await?;
          // If compressed hash matches
          if &actual_compressed_hash != expected_compressed_hash {
            let _ = fs::remove_file(compressed_target);
            return Err(DownloadError::ChecksumMismatch { expected: expected_compressed_hash.clone().into(), actual: actual_compressed_hash.into() });
          }
        }
        self.decompress_asset(&compressed_target)
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

      info!("Downloaded asset and hash matched successfully");
    } else {
      info!("Downloaded successfully asset but no checksum provided, assuming it's good");
    }

    info!("Downloaded successfully and checksum matched");
    Ok(())
  }
}

pub enum AssetDownloadableStatus {
  Downloading,
  Extracting,
}

impl AssetDownloadableStatus {
  pub fn as_str(&self) -> &str {
    match self {
      AssetDownloadableStatus::Downloading => "Downloading",
      AssetDownloadableStatus::Extracting => "Extracting",
    }
  }
}

#[derive(Debug, Error)]
pub enum CompressedAssetError {
  #[error("Failed to write compressed asset: {0}")] WriteFile(#[source] std::io::Error),
  #[error("Failed to remove compressed asset: {0}")] RemoveFile(#[source] std::io::Error),
  #[error("Failed to read compressed asset: {0}")] ReadAsset(#[source] std::io::Error),
  #[error("Couldn't prepare the destination folder for the compressed asset: {0}")] PrepareDestination(#[source] std::io::Error),
  #[error("Failed to download compressed asset: {0}")] Download(#[from] reqwest::Error),
  #[error(
    "Hash did not match downloaded compressed asset (Expected {}, downloaded {})",
    hex::encode(expected),
    hex::encode(actual)
  )] ChecksumMismatch {
    expected: Vec<u8>,
    actual: Vec<u8>,
  },
}

impl From<CompressedAssetError> for DownloadError {
  fn from(val: CompressedAssetError) -> Self {
    DownloadError::Other(Box::new(val))
  }
}
