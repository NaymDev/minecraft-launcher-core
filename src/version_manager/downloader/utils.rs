use std::{ boxed::Box, fs::{ self, create_dir_all, File }, io::{ self, Cursor }, path::Path, vec::Vec };

use log::warn;
use reqwest::{ Client, Url };
use sha1::{ Digest, Sha1 };

use crate::{
  download_utils::downloadables::{ AssetDownloadable, ChecksummedDownloadable, Downloadable, EtagDownloadable, PreHashedDownloadable },
  json::{
    manifest::{ assets::AssetIndex, download::{ DownloadInfo, DownloadType }, library::Library, rule::OperatingSystem, VersionManifest },
    EnvironmentFeatures,
    Sha1Sum,
    VersionInfo,
  },
};

pub fn get_jar_downloadable(game_dir: &Path, local_version: &VersionManifest) -> Box<dyn Downloadable + Send + Sync> {
  let version_id = local_version.get_id().to_string();
  let jar_path = game_dir.join("versions").join(&version_id).join(format!("{}.jar", &version_id));

  if let Some(DownloadInfo { sha1, url, .. }) = local_version.get_download_url(DownloadType::Client) {
    Box::new(PreHashedDownloadable::new(url, &jar_path, false, sha1.clone()))
  } else {
    let url = format!("https://s3.amazonaws.com/Minecraft.Download/versions/{}/{}.jar", &version_id, &version_id);
    Box::new(EtagDownloadable::new(&url, &jar_path, false))
  }
}

pub fn get_library_downloadables(
  game_dir: &Path,
  local_version: &VersionManifest,
  env_features: &EnvironmentFeatures,
  os: Option<OperatingSystem>
) -> Vec<Box<dyn Downloadable + Send + Sync>> {
  let os = os.unwrap_or(OperatingSystem::get_current_platform());
  local_version
    .get_relevant_libraries(env_features)
    .into_iter()
    .flat_map(|lib| create_lib_downloadable(lib, game_dir, &os, false))
    .collect()
}

pub async fn get_asset_downloadables(
  game_dir: &Path,
  local_version: &VersionManifest
) -> Result<Vec<Box<dyn Downloadable + Send + Sync>>, Box<dyn std::error::Error>> {
  let assets_dir = game_dir.join("assets");
  let objects_dir = assets_dir.join("objects");
  let indexes_dir = assets_dir.join("indexes");

  let index_info = local_version.asset_index.as_ref().ok_or("Asset index not found in version manifest!")?;
  let index_file = indexes_dir.join(format!("{}.json", index_info.id));

  if let Ok(mut file) = File::open(&index_file) {
    // Obtain the SHA-1 hash of the already downloaded index file
    let mut sha1 = Sha1::new();
    io::copy(&mut file, &mut sha1)?;
    let sha1 = Sha1Sum::new(sha1.finalize().into());

    // If the hash is incorrect, redownload
    if sha1 != index_info.sha1 {
      warn!("Asset index file is invalid, redownloading");
      fs::remove_file(&index_file)?;
    }
  }

  // Parse the asset index file
  let asset_index: AssetIndex = if let Ok(file) = File::open(&index_file) {
    serde_json::from_reader(file)?
  } else {
    // Download asset index file and parse it
    let bytes = Client::new().get(&index_info.url).send().await?.bytes().await?;
    create_dir_all(indexes_dir)?;
    fs::write(&index_file, &bytes)?;
    serde_json::from_reader(&mut Cursor::new(&bytes))?
  };

  // Turn each resource object into a downloadable
  let mut downloadables: Vec<Box<dyn Downloadable + Send + Sync>> = vec![];
  for (asset_object, asset_name) in asset_index.get_unique_objects() {
    downloadables.push(Box::new(AssetDownloadable::new(asset_name, asset_object, "https://resources.download.minecraft.net/", &objects_dir)));
  }

  Ok(downloadables)
}

pub fn create_lib_downloadable(
  lib: &Library,
  game_dir: &Path,
  os: &OperatingSystem,
  force_download: bool
) -> Option<Box<dyn Downloadable + Send + Sync>> {
  // If the lib has a natives field, but the os is not supported, return None immediately
  let classifier = lib.get_artifact_classifier(os)?;

  let libraries_dir = game_dir.join("libraries");
  let file_path = lib.name.get_local_path(&libraries_dir);
  let artifact_path = lib.get_artifact_path(classifier);

  // If the lib has a single url
  if let Some(url) = &lib.url {
    let mut url = Url::parse(url).ok()?;
    url.set_path(&artifact_path);
    let downloadable = ChecksummedDownloadable::new(url.as_str(), &file_path, force_download);
    return Some(Box::new(downloadable));
  }

  // If the lib has no url, try the default download server
  if lib.downloads.is_none() {
    let url = format!("https://libraries.minecraft.net/{}", &artifact_path);
    return Some(Box::new(ChecksummedDownloadable::new(&url, &file_path, force_download)));
  }

  // If the lib has multiple urls (like for each OS)
  // We obtain the download info for the OS
  if let Some(DownloadInfo { url, sha1, .. }) = lib.get_download_info(os) {
    let downloadable = PreHashedDownloadable::new(&url, &file_path, false, sha1);
    Some(Box::new(downloadable))
  } else {
    None
  }
}
