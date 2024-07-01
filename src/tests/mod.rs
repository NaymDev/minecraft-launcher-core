use crate::{
  bootstrap::{ auth::UserAuthentication, options::{ GameOptionsBuilder, LauncherOptions, ProxyOptions }, GameBootstrap },
  json::{ EnvironmentFeatures, MCVersion, ReleaseType, VersionInfo },
  version_manager::{ downloader::progress::{ CallbackReporter, Event, ProgressReporter }, remote::RawVersionList, VersionManager },
};

use std::{ collections::HashMap, env::temp_dir, path::PathBuf, sync::{ Arc, Mutex } };
use chrono::{ Timelike, Utc };
use futures::{ stream, StreamExt };
use log::{ debug, error, info, trace, LevelFilter };
use simple_logger::SimpleLogger;

mod minecraft_version;

pub fn setup_logger() {
  SimpleLogger::new().env().with_level(LevelFilter::Debug).init().unwrap();
}

#[tokio::test]
async fn test_version_manager() -> Result<(), Box<dyn std::error::Error>> {
  setup_logger();
  let mut version_manager = VersionManager::load(&temp_dir().join(".minecraft-test-rust"), &EnvironmentFeatures::default()).await?;
  let local = version_manager.install_version_by_id(&MCVersion::from("1.20.1".to_string())).await?;
  let resolved = version_manager.resolve_inheritances(local).await?;
  info!("Resolved: {:#?}", resolved);
  Ok(())
}

#[tokio::test]
async fn test_game() -> Result<(), Box<dyn std::error::Error>> {
  setup_logger();

  trace!("Commencing testing game");

  let game_dir = temp_dir().join(".minecraft-core-test");
  info!("Game dir: {game_dir:?}");

  info!("Attempting to launch the game");

  let progress: Arc<Mutex<Option<(String, usize, usize)>>> = Arc::new(Mutex::new(None));

  let reporter: ProgressReporter = {
    let progress = Arc::clone(&progress);

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
  };

  let java_path = PathBuf::from(env!("JAVA_HOME")).join("bin").join("java.exe");
  let reporter = Arc::new(reporter);
  let mc_version = MCVersion::new("1.20.1");

  let natives_dir = game_dir.join("versions").join(mc_version.to_string()).join(format!("{}-natives-{}", mc_version, Utc::now().nanosecond()));
  let game_options = GameOptionsBuilder::default()
    .game_dir(game_dir)
    .natives_dir(natives_dir)
    .proxy(ProxyOptions::NoProxy)
    .java_path(java_path)
    .authentication(UserAuthentication::offline("MonkeyKiller_"))
    .launcher_options(LauncherOptions::new("Test Launcher", "v1.0.0"))
    .max_concurrent_downloads(32)
    .build()?;

  reporter.setup("Fetching version manifest", Some(2));
  let env_features = game_options.env_features();
  let mut version_manager = VersionManager::load(&game_options.game_dir, &env_features).await?;

  info!("Queuing library & version downloads");
  reporter.status("Resolving local version");
  reporter.progress(1);
  let manifest = version_manager.resolve_local_version(&mc_version, true, true).await?;
  if !manifest.applies_to_current_environment(&env_features) {
    return Err(format!("Version {} is is incompatible with the current environment", mc_version).into());
  }

  reporter.done();
  version_manager.download_required_files(&manifest, game_options.max_concurrent_downloads, game_options.max_download_attempts, &reporter).await?;

  let mut game_runner = GameBootstrap::new(game_options);
  let mut process = game_runner.launch_game(&manifest).await?;
  let status = loop {
    if let Some(status) = process.exit_status() {
      break status;
    }
  };
  info!("Game exited with status: {status}");

  Ok(())
}

#[tokio::test]
async fn test_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
  setup_logger();

  let versions = RawVersionList::fetch().await?
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
      let result = remote.fetch().await;
      if result.is_ok() {
        info!(" - Parse version {} ({}/{}): SUCCESS", id, i + 1, count);
      } else {
        error!(" - Parse version {} ({}/{}): FAILED", id, i + 1, count);
      }
      (id.clone(), result)
    })
    .buffer_unordered(5)
    .collect::<Vec<_>>().await;

  let failures = results
    .into_iter()
    .filter_map(|(id, result)| result.err().map(|e| (id, e)))
    .collect::<HashMap<_, _>>();

  if failures.is_empty() {
    info!("All versions parsed successfully");
  } else {
    error!("Failed to parse {} versions: ", failures.len());
    for (id, error) in failures {
      error!("  - {id}: {error}");
    }
  }

  Ok(())
}
