use std::{ fs::{ self, File }, io::{ Cursor, Read }, path::{ PathBuf, MAIN_SEPARATOR_STR }, sync::{ Arc, Mutex } };

use async_trait::async_trait;
use libflate::non_blocking::gzip;
use log::{ info, warn };
use reqwest::{ Client, Url };

use crate::{ download_utils::{ error::Error, DownloadableMonitor }, json::{ manifest::assets::AssetObject, Sha1Sum } };

use super::Downloadable;

pub struct AssetDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub name: String,
  pub status: Mutex<AssetDownloadableStatus>,
  pub asset: AssetObject,
  pub url_base: String,
  pub destination: PathBuf,
  pub monitor: Arc<DownloadableMonitor>,
}

impl AssetDownloadable {
  pub fn new(name: &str, asset: &AssetObject, url_base: &str, objects_dir: &PathBuf) -> Self {
    let path = AssetObject::create_path_from_hash(&asset.hash);
    let mut url = Url::parse(url_base).unwrap();
    url.set_path(&path);
    let url = url.to_string();
    let target_file = objects_dir.join(path.replace("/", MAIN_SEPARATOR_STR));
    Self {
      url,
      target_file,
      force_download: false,
      attempts: Arc::new(Mutex::new(0)),
      start_time: Arc::new(Mutex::new(None)),
      end_time: Arc::new(Mutex::new(None)),

      name: name.to_string(),
      status: Mutex::new(AssetDownloadableStatus::Downloading),
      asset: asset.clone(),
      url_base: url_base.to_string(),
      destination: objects_dir.clone(),
      monitor: Arc::new(DownloadableMonitor::new(0, 5242880)),
    }
  }

  fn decompress_asset(&self, target: &PathBuf, compressed_target: &PathBuf) -> Result<(), Error> {
    if let Ok(mut status) = self.status.lock() {
      *status = AssetDownloadableStatus::Extracting;
    }
    let reader = &mut File::open(compressed_target)?;
    let mut decoder = gzip::Decoder::new(reader);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    fs::write(target, &bytes)?;

    let local_sha1 = Sha1Sum::from_reader(&mut Cursor::new(&bytes))?;
    if local_sha1 == self.asset.hash {
      info!("Had local compressed asset, unpacked successfully and hash matched");
    } else {
      fs::remove_file(target)?;
      return Err(
        Error::Other(format!("Had local compressed asset but unpacked hash did not match (expected {}, but had {})", self.asset.hash, local_sha1))
      );
    }
    Ok(())
  }
}

#[async_trait]
impl Downloadable for AssetDownloadable {
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
    format!("{} {}", self.status.lock().unwrap().as_str(), self.name)
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

  async fn download(&self, client: &Client) -> Result<(), Error> {
    if let Ok(mut attempts) = self.attempts.lock() {
      *attempts += 1;
    }

    if let Ok(mut status) = self.status.lock() {
      *status = AssetDownloadableStatus::Downloading;
    }

    let target = self.get_target_file();
    let compressed_target = if self.asset.has_compressed_alternative() {
      Some(self.destination.join(AssetObject::create_path_from_hash(self.asset.compressed_hash.as_ref().unwrap())))
    } else {
      None
    };
    let url = self.url();
    let compressed_url = if self.asset.has_compressed_alternative() {
      let mut url = Url::parse(&self.url_base).map_err(|_| Error::UrlParseError(self.url_base.clone()))?;
      url.set_path(&AssetObject::create_path_from_hash(self.asset.compressed_hash.as_ref().unwrap()));
      Some(url.to_string())
    } else {
      None
    };
    self.ensure_file_writable(target)?;
    if let Some(compressed_target) = &compressed_target {
      self.ensure_file_writable(&compressed_target)?;
    }

    if target.is_file() {
      let file_len = target.metadata()?.len();
      if file_len == self.asset.size {
        info!("Have local file and it's the same size; assuming it's okay!");
        return Ok(());
      }

      warn!("Had local file but it was the wrong size... had {} but expected {}", file_len, self.asset.size);
      fs::remove_file(target)?;
    }

    if let Some(compressed_target) = &compressed_target {
      if compressed_target.is_file() {
        let local_hash = Sha1Sum::from_reader(&mut File::open(compressed_target)?)?;
        if &local_hash == self.asset.compressed_hash.as_ref().unwrap() {
          return self.decompress_asset(target, &compressed_target);
        }

        warn!("Had local compressed but it was the wrong hash... expected {} but had {}", self.asset.compressed_hash.as_ref().unwrap(), local_hash);
        fs::remove_file(compressed_target)?;
      }
    }

    if let (Some(compressed_url), Some(compressed_target)) = (&compressed_url, &compressed_target) {
      let res = client.get(compressed_url).send().await?.error_for_status()?;
      if let Some(content_len) = res.content_length() {
        self.monitor.set_total(content_len as usize);
      }
      let bytes = res.bytes().await?;
      fs::write(compressed_target, &bytes)?;
      let local_hash = Sha1Sum::from_reader(&mut Cursor::new(&bytes))?;
      if &local_hash == self.asset.compressed_hash.as_ref().unwrap() {
        return self.decompress_asset(target, &compressed_target);
      } else {
        fs::remove_file(&compressed_target)?;
        return Err(
          Error::Other(
            format!(
              "Hash did not match downloaded compressed asset (Expected {}, downloaded {})",
              self.asset.compressed_hash.as_ref().unwrap(),
              local_hash
            )
          )
        );
      }
    } else {
      let res = client.get(url).send().await?.error_for_status()?;
      if let Some(content_len) = res.content_length() {
        self.monitor.set_total(content_len as usize);
      }
      let bytes = res.bytes().await?;
      fs::write(target, &bytes)?;
      let local_hash = Sha1Sum::from_reader(&mut Cursor::new(&bytes))?;
      if local_hash == self.asset.hash {
        info!("Downloaded asset and hash matched successfully");
        return Ok(());
      } else {
        fs::remove_file(target)?;
        Err(Error::Other(format!("Hash did not match downloaded asset (Expected {}, downloaded {})", self.asset.hash, local_hash)))
      }
    }
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
