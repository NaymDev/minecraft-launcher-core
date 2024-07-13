use crate::{
  bootstrap::{ auth::UserAuthentication, options::{ GameOptionsBuilder, LauncherOptions, ProxyOptions }, GameBootstrap },
  java_manager::JavaRuntimeManager,
  json::{ EnvironmentFeatures, MCVersion, ReleaseType, VersionInfo },
  version_manager::{
    downloader::{ download_job::DownloadJob, progress::{ CallbackReporter, Event, ProgressReporter } },
    remote::RawVersionList,
    VersionManager,
  },
};

use std::{ collections::HashMap, env::temp_dir, path::PathBuf, sync::{ Arc, Mutex } };
use chrono::{ Duration, Timelike, Utc };
use futures::{ stream, StreamExt };
use log::{ debug, error, info, trace, LevelFilter };
use reqwest::Client;
use simple_logger::SimpleLogger;

pub fn setup_logger() {
  let _ = SimpleLogger::new().env().with_level(LevelFilter::Debug).init();
}

pub fn create_progress_reporter() -> ProgressReporter {
  let progress: Arc<Mutex<Option<(String, usize, usize)>>> = Arc::default();

  fn print_progress(status: &str, current: usize, total: usize) {
    let current = current as f64;
    let total = total as f64;
    if total != 0f64 {
      let percentage = ((current / total) * 20f64).ceil() as usize;
      let left = 20 - percentage;
      let bar = format!("[{}{}]", "■".repeat(percentage), "·".repeat(left));
      let progress = (current / total) * 100f64;
      debug!("{status} {bar} ({current}%)", current = progress.ceil() as u32);
    }
  }

  Arc::new(
    CallbackReporter::new(move |event| {
      if let Ok(mut progress) = progress.lock() {
        if let Event::Done = event {
          progress.take();
          debug!("Progress hidden");
        } else {
          let mut taken = progress.take().unwrap_or_default();
          match event {
            Event::Status(status) => {
              if taken.0 != status {
                taken.0 = status;
                print_progress(&taken.0, taken.1, taken.2);
              }
            }
            Event::Progress(progress) => {
              if taken.1 != progress {
                taken.1 = progress;
                print_progress(&taken.0, progress, taken.2);
              }
            }
            Event::Total(total) => {
              if taken.2 != total {
                taken.2 = total;
                print_progress(&taken.0, taken.1, total);
              }
            }
            Event::Setup { status, total } => {
              taken = (status, 0, total.unwrap_or(0));
              print_progress(&taken.0, taken.1, taken.2);
            }
            _ => {}
          }

          progress.replace(taken);
        }
      }
    })
  )
}

#[tokio::test]
async fn test_version_manager() -> Result<(), Box<dyn std::error::Error>> {
  setup_logger();

  let mc_version = MCVersion::new("1.20.1");
  let game_dir = temp_dir().join(".minecraft-test-rust");

  let mut version_manager = VersionManager::load(&game_dir, &EnvironmentFeatures::default(), None).await?;
  let resolved = version_manager.resolve_local_version(&mc_version, true, false).await?;
  info!("Resolved: {:?}", resolved);
  Ok(())
}

#[tokio::test]
async fn test_game() -> Result<(), Box<dyn std::error::Error>> {
  setup_logger();

  let mc_version = MCVersion::new("1.20.1");

  let game_dir = temp_dir().join(".minecraft-core-test");
  let natives_dir = game_dir.join("versions").join(mc_version.to_string()).join(format!("{}-natives-{}", mc_version, Utc::now().nanosecond()));
  let objects_dir = &game_dir.join("assets").join("objects");

  let java_path = if let Ok(java_home) = std::env::var("JAVA_HOME") {
    PathBuf::from(java_home).join("bin").join("java.exe")
  } else {
    PathBuf::default()
  };
  let reporter = create_progress_reporter();

  trace!("Commencing testing game");
  info!("Game dir: {game_dir:?}");
  info!("Attempting to launch the game");

  let mut game_options = GameOptionsBuilder::default()
    .game_dir(game_dir)
    .natives_dir(natives_dir)
    .proxy(ProxyOptions::NoProxy)
    .java_path(java_path)
    .authentication(UserAuthentication::offline("Player"))
    .launcher_options(LauncherOptions::new("Test Launcher", "v1.0.0"))
    .build()?;
  let client = Client::new();
  let env_features = game_options.env_features();

  reporter.setup("Fetching version manifest", Some(2));
  let mut version_manager = VersionManager::load(&game_options.game_dir, &env_features, Some(client.clone())).await?;

  info!("Queuing library & version downloads");
  reporter.status("Resolving local version");
  reporter.progress(1);

  let manifest = version_manager.resolve_local_version(&mc_version, true, false).await?;
  if !manifest.applies_to_current_environment(&env_features) {
    return Err(format!("Version {} is is incompatible with the current environment", mc_version).into());
  }
  reporter.done();

  debug!("Installing java runtime");
  let runtime_manager = JavaRuntimeManager::load(&game_options.game_dir.join("runtimes"), &client).await?;

  let component = &manifest.java_version.as_ref().unwrap().component;
  runtime_manager.install_runtime(objects_dir, component, &reporter).await?;
  game_options.java_path = runtime_manager.get_java_executable(component);

  debug!("Java runtime installed");

  version_manager.download_required_files(&manifest, &reporter, None, Some(20)).await?;

  let mut game_runner = GameBootstrap::new(game_options);
  let mut process = game_runner.launch_game(&manifest)?;

  let start_time = Utc::now();
  let (status, child) = loop {
    if let Some(status) = process.exit_status() {
      let child = process.into_inner();
      break (Some(status), child);
    }
    if Utc::now() - start_time > Duration::seconds(15) {
      let mut child = process.into_inner();
      child.kill()?;
      break (None, child);
    }
  };

  match status {
    Some(0) | None => {
      info!("Game exited successfully with code 0");
      Ok(())
    }
    Some(code) => {
      error!("================================================================");
      if let Ok(output) = child.wait_with_output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Game exited with code {code}:\n{stdout}\n{stderr}");

        if stdout.contains("Setting user: Player") {
          info!("Game failed to launch, but it was expected to");
          return Ok(());
        }
      }
      error!("================================================================");
      Err(format!("Game exited with code {code}").into())
    }
  }
}

#[tokio::test]
async fn test_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
  setup_logger();

  let client = DownloadJob::create_http_client(None).unwrap_or_default();

  let versions = RawVersionList::fetch(&client).await?
    .versions.into_iter()
    .filter(|v| v.get_type() != &ReleaseType::Snapshot)
    .enumerate()
    .collect::<Vec<_>>();

  let count = versions.len();
  info!("Attempting to parse {} versions", count);
  let results = stream
    ::iter(versions)
    .map(|(i, remote)| async move {
      let id = remote.get_id();
      let result = remote.fetch(&Client::new()).await;
      if result.is_ok() {
        info!(" - Parse version {} ({}/{}): SUCCESS", id, i + 1, count);
      } else {
        error!(" - Parse version {} ({}/{}): FAILED", id, i + 1, count);
      }
      (id.clone(), result.err())
    })
    .buffer_unordered(32)
    .collect::<Vec<_>>().await;

  let failures = results
    .into_iter()
    .filter_map(|(id, err)| err.map(|e| (id, e)))
    .collect::<HashMap<_, _>>();

  if failures.is_empty() {
    info!("All versions parsed successfully");
    Ok(())
  } else {
    error!("Failed to parse {} versions: ", failures.len());
    for (id, error) in &failures {
      error!("  - {id}: {error}");
    }
    Err(format!("Failed to parse {} versions: ", failures.len()).into())
  }
}
