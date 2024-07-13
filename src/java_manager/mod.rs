use std::{ env::consts::ARCH, fs, io, path::{ Path, PathBuf } };

use downloadable::RuntimeFileDownloadable;
use error::InstallRuntimeError;
use index::{ JreIndex, RuntimeInfo };
use log::{ debug, error };
use manifest::{ JavaRuntimeFile, JreManifest };
use md5::Digest;
use reqwest::Client;
use sha1::Sha1;

use crate::{
  json::{ manifest::rule::OperatingSystem, Sha1Sum },
  version_manager::downloader::{ download_job::DownloadJob, downloadables::Downloadable, progress::ProgressReporter },
};

pub mod index;
pub mod manifest;
pub mod downloadable;

pub mod error;

#[derive(Debug)]
pub struct JavaRuntimeManager {
  pub runtimes_dir: PathBuf,
  pub client: Client,
  pub jre_manifest: Option<JreIndex>,
  pub os: OperatingSystem,
  pub arch: String,
}

impl JavaRuntimeManager {
  pub fn new(runtimes_dir: &Path, client: &Client) -> Self {
    Self {
      runtimes_dir: runtimes_dir.to_path_buf(),
      client: client.clone(),
      jre_manifest: None,

      os: OperatingSystem::get_current_platform(),
      arch: ARCH.to_string(),
    }
  }

  pub async fn load(runtimes_dir: &Path, client: &Client) -> Result<Self, reqwest::Error> {
    let mut manager = Self::new(runtimes_dir, client);
    manager.refresh().await?;
    Ok(manager)
  }

  pub async fn refresh(&mut self) -> Result<(), reqwest::Error> {
    self.jre_manifest = Some(JreIndex::fetch(&self.client).await?);
    Ok(())
  }
}

impl JavaRuntimeManager {
  pub fn get_installed_runtimes(&self) -> Result<Vec<String>, io::Error> {
    Ok(
      self.runtimes_dir
        .read_dir()?
        .flatten()
        .map(|e| e.file_name())
        .flat_map(|s| s.to_str().map(|s| s.to_string()))
        .collect()
    )
  }

  pub async fn install_runtime(&self, objects_dir: &Path, component: &str, reporter: &ProgressReporter) -> Result<(), InstallRuntimeError> {
    if let Some(info) = &self.jre_manifest {
      let entry = info.find(&self.os, Some(&self.arch)).ok_or(InstallRuntimeError::UnsupportedOS)?;
      let runtimes = entry.get(component).ok_or(InstallRuntimeError::RuntimeNotFound { component: component.to_string() })?;

      for runtime in runtimes {
        if let Err(err) = self.try_install_runtime(objects_dir, component, reporter, runtime).await {
          error!("Failed to install runtime: {}", err);
          continue;
        }
        return Ok(());
      }

      Err(InstallRuntimeError::InstallFailure)
    } else {
      Err(InstallRuntimeError::RuntimeNotFound { component: component.to_string() })
    }
  }

  pub async fn try_install_runtime(
    &self,
    objects_dir: &Path,
    component: &str,
    reporter: &ProgressReporter,
    runtime_info: &RuntimeInfo
  ) -> Result<(), InstallRuntimeError> {
    let RuntimeInfo { manifest, version: jre_version_info, .. } = runtime_info;

    debug!("Downloading {}", manifest.url);
    let manifest: JreManifest = {
      let mut bytes = vec![];
      let mut response = self.client.get(&manifest.url).send().await?.error_for_status()?;
      let mut sha1 = Sha1::new();
      while let Some(chunk) = response.chunk().await? {
        bytes.extend_from_slice(&chunk);
        sha1.update(&chunk);
      }
      let sha1 = Sha1Sum::from(sha1);
      if sha1 != manifest.sha1 {
        return Err(InstallRuntimeError::ChecksumMismatch { expected: manifest.sha1.clone(), actual: sha1 });
      }
      serde_json::from_slice(&bytes)?
    };

    let runtime_dir = self.get_runtime_dir(component);
    debug!("Downloaded java manifest. Installing {}", runtime_dir.display());

    fs::create_dir_all(&runtime_dir).map_err(|err| InstallRuntimeError::CreateFolder { folder: runtime_dir.clone(), source: err })?;
    let job = DownloadJob::new("Java Runtime").with_progress_reporter(reporter).with_client(self.client.clone());
    let mut downloadables: Vec<Box<dyn Downloadable + Send + Sync>> = vec![];
    for (name, file) in &manifest.files {
      let target = runtime_dir.join(name);
      if let JavaRuntimeFile::File { downloads, executable } = file {
        let name = name.to_string_lossy().to_string();
        let downloadable = RuntimeFileDownloadable::new(&name, downloads.clone(), *executable, objects_dir, &target);
        downloadables.push(Box::new(downloadable));
      }
    }
    debug!("Starting java download");
    job.add_downloadables(downloadables).start().await?;
    fs::write(runtime_dir.join(".version"), &jre_version_info.name).map_err(InstallRuntimeError::WriteVersionFile)?;

    for (name, file) in manifest.files {
      let target = runtime_dir.join(&name);
      match file {
        JavaRuntimeFile::Directory => {
          fs::create_dir_all(&target).map_err(|err| InstallRuntimeError::CreateFolder {
            folder: target.clone(),
            source: err,
          })?;
        }
        #[cfg(target_os = "linux")]
        JavaRuntimeFile::Link { target: link_target } => {
          use std::os::unix::fs::symlink;

          let link_target = PathBuf::from(link_target);

          if !target.exists() {
            if let Some(parent) = target.parent() {
              fs::create_dir_all(parent).map_err(|err| InstallRuntimeError::CreateFolder {
                folder: target.clone(),
                source: err,
              })?;
            }
            symlink(&link_target, &target).map_err(|err| {
              InstallRuntimeError::CreateSymlink {
                file: target.clone(),
                source: err,
              }
            })?;
          }
        }
        _ => {}
      }
    }

    Ok(())
  }

  pub fn get_runtime_dir(&self, component: &str) -> PathBuf {
    let platform = jvm_platform_string(&self.os, Some(&self.arch));
    self.runtimes_dir.join(component).join(platform)
  }

  pub fn get_platform_name() -> String {
    jvm_platform_string(&OperatingSystem::get_current_platform(), Some(ARCH))
  }

  pub fn get_java_executable(&self, component: &str) -> PathBuf {
    let os = &self.os;
    let runtime_dir = self.get_runtime_dir(component);

    if component == "minecraft-java-exe" {
      runtime_dir.join("MinecraftJava.exe")
    } else if os == &OperatingSystem::Windows {
      runtime_dir.join("bin").join("javaw.exe")
    } else if os == &OperatingSystem::Osx {
      runtime_dir.join("jre.bundle").join("Contents").join("Home").join("bin").join("java")
    } else {
      runtime_dir.join("bin").join("java")
    }
  }

  pub fn jvm_platform_string(os: &OperatingSystem, arch: Option<&str>) -> String {
    let os = match os {
      OperatingSystem::Linux => "linux",
      OperatingSystem::Windows => "windows",
      OperatingSystem::Osx => "mac-os",
      _ => {
        return "gamecore".to_owned();
      }
    };

    let arch = match arch {
      Some("x86_64") => Some("x64"),
      Some("x64") | Some("x86") | Some("i386") | Some("arm64") | None => arch,
      _ => None,
    };

    if let Some(arch) = arch {
      format!("{os}-{arch}")
    } else {
      os.to_owned()
    }
  }
}

pub fn jvm_platform_string(os: &OperatingSystem, arch: Option<&str>) -> String {
  let os = match os {
    OperatingSystem::Linux => "linux",
    OperatingSystem::Windows => "windows",
    OperatingSystem::Osx => "mac-os",
    _ => {
      return "gamecore".to_owned();
    }
  };

  let arch = match arch {
    Some("x86_64") => Some("x64"),
    Some("x64") | Some("x86") | Some("i386") | Some("arm64") | None => arch,
    _ => None,
  };

  if let Some(arch) = arch {
    format!("{os}-{arch}")
  } else {
    os.to_owned()
  }
}

#[cfg(test)]
mod tests {
  use std::{ env::temp_dir, sync::Arc };

  use crate::{ tests::setup_logger, version_manager::downloader::progress::CallbackReporter };

  use super::*;

  #[tokio::test]
  async fn test_load() {
    setup_logger();
    let client = Client::new();
    let game_dir = temp_dir().join(".minecraft-launcher-core");
    let objects_dir = game_dir.join("assets").join("objects");
    let manager = JavaRuntimeManager::load(&game_dir.join("runtimes"), &client).await.unwrap();
    println!("{:#?}", manager);
    let reporter: ProgressReporter = Arc::new(CallbackReporter::new(|_| {}));
    manager.install_runtime(&objects_dir, "jre-legacy", &reporter).await.unwrap();
  }
}
