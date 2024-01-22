pub mod download_job;

use std::{
  path::{ PathBuf, MAIN_SEPARATOR_STR },
  io::{ Cursor, Read },
  fs::{ create_dir_all, self, File },
  time::Duration,
  sync::{ Mutex, Arc },
  ffi::OsStr,
};

use async_trait::async_trait;
use libflate::non_blocking::gzip;
use log::{ info, warn };
use reqwest::{ Client, Proxy, Url, header::HeaderValue };

use crate::{ versions::json::{ Sha1Sum, AssetObject }, MinecraftLauncherError, progress_reporter::ProgressReporter };

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
}

#[async_trait]
pub trait Downloadable {
  fn get_proxy(&self) -> &ProxyOptions;
  fn url(&self) -> &String;
  fn get_target_file(&self) -> &PathBuf;
  fn force_download(&self) -> bool;
  fn get_attempts(&self) -> usize;

  fn get_status(&self) -> String;
  fn get_monitor(&self) -> &Arc<DownloadableMonitor>;

  fn get_start_time(&self) -> Option<u64>;
  fn set_start_time(&self, start_time: u64);
  fn get_end_time(&self) -> Option<u64>;
  fn set_end_time(&self, end_time: u64);

  async fn make_connection(&self, url: &str) -> reqwest::Result<reqwest::Response> {
    let client = self.get_proxy().client_builder().timeout(Duration::from_secs(15)).build()?;
    client
      .get(url)
      .header("Cache-Control", "no-store,max-age=0,no-cache")
      .header("Expires", "0")
      .header("Pragma", "no-cache")
      .send().await?
      // TODO: CHANGE and handle for each downloadable
      .error_for_status()
  }

  fn ensure_file_writable(&self, file: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // let target = self.get_target_file();
    if let Some(parent) = file.parent() {
      if !parent.is_dir() {
        info!("Making directory {}", parent.display());
        create_dir_all(parent)?;
      }
    }

    Ok(())
  }

  async fn download(&self) -> Result<(), Box<dyn std::error::Error + 'life0>>;
}

pub struct SimpleDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub proxy: ProxyOptions,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
}

// Checksummed downloadable
pub struct ChecksummedDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub proxy: ProxyOptions,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub monitor: Arc<DownloadableMonitor>,
}

impl ChecksummedDownloadable {
  pub fn new(proxy: ProxyOptions, url: &str, target_file: &PathBuf, force_download: bool) -> Self {
    Self {
      url: url.to_string(),
      target_file: target_file.to_path_buf(),
      proxy,
      force_download,
      attempts: Arc::new(Mutex::new(0)),
      start_time: Arc::new(Mutex::new(None)),
      end_time: Arc::new(Mutex::new(None)),

      monitor: Arc::new(DownloadableMonitor::new(0, 5242880)),
    }
  }

  const NULL_SHA1: [u8; 20] = [0; 20];

  async fn get_remote_hash(&self) -> Result<Sha1Sum, Box<dyn std::error::Error>> {
    let res = self.make_connection(&format!("{}.sha1", self.url)).await?;
    Ok(Sha1Sum::try_from(res.text().await?)?)
  }
}

#[async_trait]
impl Downloadable for ChecksummedDownloadable {
  fn get_proxy(&self) -> &ProxyOptions {
    &self.proxy
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

    let mut local_hash = None;
    let mut expected_hash = None;

    self.ensure_file_writable(&self.target_file)?;
    let target_file = self.get_target_file();

    // Try to get hash from local file
    if local_hash.is_none() && target_file.is_file() {
      local_hash = Some(Sha1Sum::from_reader(&mut File::open(target_file)?)?);
    }

    if expected_hash.is_none() {
      expected_hash = Some(self.get_remote_hash().await.unwrap_or(Sha1Sum::new(Self::NULL_SHA1)));
    }

    if expected_hash.as_ref().unwrap() == &Sha1Sum::new(Self::NULL_SHA1) && target_file.is_file() {
      info!("Couldn't find a checksum so assuming our copy is good");
      return Ok(());
    } else if expected_hash == local_hash {
      info!("Remote checksum matches local file");
      return Ok(());
    } else {
      let res = self.make_connection(&self.url).await?;
      if let Some(content_len) = res.content_length() {
        self.monitor.set_total(content_len as usize);
      }
      let bytes = res.bytes().await?;
      local_hash = Some(Sha1Sum::from_reader(&mut Cursor::new(&bytes))?);
      fs::write(&target_file, &bytes)?;
      if expected_hash.as_ref().unwrap() == &Sha1Sum::new(Self::NULL_SHA1) {
        info!("Didn't have checksum so assuming the downloaded file is good");
        return Ok(());
      } else if expected_hash == local_hash {
        info!("Downloaded successfully and checksum matched");
        return Ok(());
      } else {
        Err(
          Box::new(
            std::io::Error::new(
              std::io::ErrorKind::Other,
              format!("Checksum did not match downloaded file (Checksum was {}, downloaded {})", expected_hash.unwrap(), local_hash.unwrap())
            )
          )
        )?;
      }
    }
    Ok(())
  }
}

pub struct PreHashedDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub proxy: ProxyOptions,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub expected_hash: Sha1Sum,
  pub monitor: Arc<DownloadableMonitor>,
}

impl PreHashedDownloadable {
  pub fn new(proxy: ProxyOptions, url: &str, target_file: &PathBuf, force_download: bool, expected_hash: Sha1Sum) -> Self {
    Self {
      url: url.to_string(),
      target_file: target_file.to_path_buf(),
      proxy,
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
  fn get_proxy(&self) -> &ProxyOptions {
    &self.proxy
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

pub struct EtagDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub proxy: ProxyOptions,
  pub force_download: bool,
  pub attempts: Arc<Mutex<usize>>,
  pub start_time: Arc<Mutex<Option<u64>>>,
  pub end_time: Arc<Mutex<Option<u64>>>,

  pub monitor: Arc<DownloadableMonitor>,
}

impl EtagDownloadable {
  pub fn new(proxy: ProxyOptions, url: &str, target_file: &PathBuf, force_download: bool) -> Self {
    Self {
      url: url.to_string(),
      target_file: target_file.to_path_buf(),
      proxy,
      force_download,
      attempts: Arc::new(Mutex::new(0)),
      start_time: Arc::new(Mutex::new(None)),
      end_time: Arc::new(Mutex::new(None)),

      monitor: Arc::new(DownloadableMonitor::new(0, 5242880)),
    }
  }

  fn get_etag(etag: Option<&HeaderValue>) -> String {
    let etag = etag.and_then(|v| v.to_str().ok());
    if let Some(etag) = etag {
      if etag.starts_with("\"") && etag.ends_with("\"") {
        return etag[1..etag.len() - 1].to_string();
      } else {
        return etag.to_string();
      }
    } else {
      "-".to_string()
    }
  }
}

#[async_trait]
impl Downloadable for EtagDownloadable {
  fn get_proxy(&self) -> &ProxyOptions {
    &self.proxy
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

    let target = &self.target_file;
    let res = self.make_connection(&self.url).await?;
    if let Some(content_len) = res.content_length() {
      self.monitor.set_total(content_len as usize);
    }
    let etag = Self::get_etag(res.headers().get("ETag"));
    let bytes = res.bytes().await?;
    let md5 = md5::compute(&bytes).0;
    fs::write(&target, &bytes)?;
    if etag.contains("-") {
      info!("Didn't have etag so assuming our copy is good");
      return Ok(());
    } else if etag.as_bytes() == &md5 {
      info!("Downloaded successfully and etag matched");
      return Ok(());
    } else {
      Err(
        Box::new(
          std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Couldn't connect to server (responded with {}), but have local file, assuming it's good", etag)
          )
        )
      )?;
    }
    Ok(())
  }
}

pub struct AssetDownloadable {
  pub url: String,
  pub target_file: PathBuf,
  pub proxy: ProxyOptions,
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
  pub fn new(proxy: ProxyOptions, name: &str, asset: &AssetObject, url_base: &str, objects_dir: &PathBuf) -> Self {
    let path = AssetObject::create_path_from_hash(&asset.hash);
    let mut url = Url::parse(url_base).unwrap();
    url.set_path(&path);
    let url = url.to_string();
    let target_file = objects_dir.join(path.replace("/", MAIN_SEPARATOR_STR));
    Self {
      proxy,
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

  fn decompress_asset(&self, target: &PathBuf, compressed_target: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
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
      Err(
        MinecraftLauncherError(
          format!("Had local compressed asset but unpacked hash did not match (expected {}, but had {})", self.asset.hash, local_sha1)
        )
      )?;
    }
    Ok(())
  }
}

#[async_trait]
impl Downloadable for AssetDownloadable {
  fn get_proxy(&self) -> &ProxyOptions {
    &self.proxy
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

  async fn download(&self) -> Result<(), Box<dyn std::error::Error + 'life0>> {
    *self.attempts.lock()? += 1;
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
      let mut url = Url::parse(&self.url_base)?;
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
      let res = self.make_connection(&compressed_url).await?;
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
        Err(
          MinecraftLauncherError(
            format!(
              "Hash did not match downloaded compressed asset (Expected {}, downloaded {})",
              self.asset.compressed_hash.as_ref().unwrap(),
              local_hash
            )
          )
        )?;
      }
    } else {
      let res = self.make_connection(&url).await?;
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
        Err(MinecraftLauncherError(format!("Hash did not match downloaded asset (Expected {}, downloaded {})", self.asset.hash, local_hash)))?;
      }
    }

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
