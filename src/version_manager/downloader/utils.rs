use std::{ boxed::Box, path::Path, str::FromStr, vec::Vec };

use reqwest::Url;

use crate::{
  version_manager::downloader::downloadables::{ AssetDownloadable, ChecksummedDownloadable, Downloadable, EtagDownloadable, PreHashedDownloadable },
  json::{
    manifest::{ assets::AssetIndex, download::{ DownloadInfo, DownloadType }, library::Library, rule::OperatingSystem, VersionManifest },
    EnvironmentFeatures,
    VersionInfo,
  },
};

pub fn get_jar_downloadable(game_dir: &Path, local_version: &VersionManifest) -> Box<dyn Downloadable + Send + Sync> {
  let version_id = local_version.get_id().to_string();
  let jar_path = game_dir.join("versions").join(&version_id).join(format!("{}.jar", &version_id));

  if let Some(DownloadInfo { sha1, url, .. }) = local_version.get_download_url(DownloadType::Client) {
    Box::new(PreHashedDownloadable::new(url, &jar_path, sha1.clone()))
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
    .flat_map(|lib| create_lib_downloadable(lib, game_dir, &os))
    .collect()
}

pub fn get_asset_downloadables(game_dir: &Path, asset_index: &AssetIndex) -> Vec<Box<dyn Downloadable + Send + Sync>> {
  let assets_dir = game_dir.join("assets");
  let objects_dir = assets_dir.join("objects");

  // Turn each resource object into a downloadable
  let url_base = Url::from_str("https://resources.download.minecraft.net/").unwrap();
  let mut downloadables: Vec<Box<dyn Downloadable + Send + Sync>> = vec![];
  for (asset_object, asset_name) in asset_index.get_unique_objects() {
    downloadables.push(Box::new(AssetDownloadable::new(asset_name, asset_object, &url_base, &objects_dir)));
  }
  downloadables
}

pub fn create_lib_downloadable(lib: &Library, game_dir: &Path, os: &OperatingSystem) -> Option<Box<dyn Downloadable + Send + Sync>> {
  // If the lib has a natives field, but the os is not supported, return None immediately
  let classifier = lib.get_artifact_classifier(os)?;

  let libraries_dir = game_dir.join("libraries");
  let artifact_path = lib.get_artifact_path(classifier);
  let file_path = libraries_dir.join(&artifact_path);

  // If the lib has a single url
  if let Some(url) = &lib.url {
    let mut url = Url::parse(url).ok()?;
    url.set_path(&artifact_path);
    let downloadable = ChecksummedDownloadable::new(url.as_str(), &file_path);
    return Some(Box::new(downloadable));
  }

  // If the lib has no url, try the default download server
  if lib.downloads.is_none() {
    let url = format!("https://libraries.minecraft.net/{}", &artifact_path);
    return Some(Box::new(ChecksummedDownloadable::new(&url, &file_path)));
  }

  // If the lib has multiple urls (like for each OS)
  // We obtain the download info for the OS
  if let Some(DownloadInfo { url, sha1, .. }) = lib.get_download_info(os) {
    let downloadable = PreHashedDownloadable::new(&url, &file_path, sha1);
    Some(Box::new(downloadable))
  } else {
    None
  }
}
