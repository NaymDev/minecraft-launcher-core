use crate::{
  bootstrap::{ auth::UserAuthentication, options::{ GameOptionsBuilder, LauncherOptions, ProxyOptions }, GameBootstrap },
  json::MCVersion,
  version_manager::{ downloader::progress_reporter::{ ProgressReporter, ProgressUpdate }, VersionManager },
};

use std::{ env::temp_dir, path::PathBuf, sync::{ Mutex, Arc } };
use chrono::{ Timelike, Utc };
use log::{ debug, info, trace, LevelFilter };
use log4rs::{
  config::{ Appender, Root, Logger },
  append::{
    console::ConsoleAppender,
    rolling_file::{ RollingFileAppender, policy::compound::{ CompoundPolicy, trigger::Trigger, roll::fixed_window::FixedWindowRoller } },
  },
  encode::pattern::PatternEncoder,
  Config,
};

#[derive(Debug)]
struct StartupTrigger {
  ran: Mutex<bool>,
}

impl StartupTrigger {
  fn new() -> Self {
    Self {
      ran: Mutex::new(false),
    }
  }
}

impl Trigger for StartupTrigger {
  fn trigger(&self, file: &log4rs::append::rolling_file::LogFile) -> anyhow::Result<bool> {
    if *self.ran.lock().unwrap() {
      Ok(false)
    } else {
      *self.ran.lock().unwrap() = true;
      Ok(file.len_estimate() > 0)
    }
  }
}

#[tokio::test]
async fn test_game() -> Result<(), Box<dyn std::error::Error>> {
  let stdout = ConsoleAppender::builder()
    .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} | {({l}):5.5} | {f}:{L} — {m}{n}")))
    .build();
  let mclc_stdout = ConsoleAppender::builder()
    .encoder(Box::new(PatternEncoder::new("[{d(%Y-%m-%d %H:%M:%S)} {l}]: {m}{n}")))
    .build();

  let date = Utc::now().format("%Y-%m-%d").to_string();

  let mclc_rolling_file = RollingFileAppender::builder()
    .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} | {({l}):5.5} | {f}:{L} — {m}{n}")))
    .build(
      "log/latest.log",
      Box::new(
        CompoundPolicy::new(
          Box::new(StartupTrigger::new()),
          // Box::new(SizeTrigger::new(10 * 1024 * 1024)), // 10 MB
          Box::new(FixedWindowRoller::builder().build(&format!("log/{date}-{{}}.log.gz"), 3).unwrap())
        )
      )
    )?;

  let minecraft_launcher_core = Logger::builder()
    .appender("mclc_stdout")
    .appender("mclc_rolling_file")
    .additive(false)
    .build("minecraft_launcher_core", LevelFilter::Debug);

  let config = Config::builder()
    .appender(Appender::builder().build("stdout", Box::new(stdout)))
    .appender(Appender::builder().build("mclc_stdout", Box::new(mclc_stdout)))
    .appender(Appender::builder().build("mclc_rolling_file", Box::new(mclc_rolling_file)))
    .logger(minecraft_launcher_core)
    .build(Root::builder().appender("stdout").build(LevelFilter::Debug))?;

  log4rs::init_config(config).unwrap();

  trace!("Commencing testing game");

  let game_dir = temp_dir().join(".minecraft-core-test");
  info!("Game dir: {game_dir:?}");

  info!("Attempting to launch the game");

  let progress: Arc<Mutex<Option<(String, u32, u32)>>> = Arc::new(Mutex::new(None));

  let reporter = {
    let progress = Arc::clone(&progress);
    ProgressReporter::new(move |update| {
      if let Ok(mut progress) = progress.lock() {
        if let ProgressUpdate::Clear = update {
          progress.take();
          debug!("Progress hidden");
        } else {
          let mut taken = progress.take().unwrap_or_default();
          match update {
            ProgressUpdate::SetStatus(status) => {
              taken.0 = status;
            }
            ProgressUpdate::SetProgress(progress) => {
              taken.1 = progress;
            }
            ProgressUpdate::SetTotal(total) => {
              taken.2 = total;
            }
            ProgressUpdate::SetAll(status, progress, total) => {
              taken = (status, progress, total);
            }
            _ => {}
          }
          if taken.2 != 0 {
            let percentage = (((taken.1 as f64) / (taken.2 as f64)) * 20f64).ceil() as usize;
            let left = 20 - percentage;
            let bar = format!("[{}{}]", "■".repeat(percentage), "·".repeat(left));
            debug!("{status} {bar} ({progress}%)", status = taken.0, progress = (((taken.1 as f64) / (taken.2 as f64)) * 100f64).ceil() as u32);
          }
          progress.replace(taken);
        }
      }
    })
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

  reporter.set("Fetching version manifest", 0, 2);
  let env_features = game_options.env_features();
  let mut version_manager = VersionManager::new(game_options.game_dir.clone(), env_features.clone()).await?;

  info!("Queuing library & version downloads");
  reporter.set_status("Resolving local version").set_progress(1);
  let manifest = version_manager.resolve_local_version(&mc_version, true, true).await?;
  if !manifest.applies_to_current_environment(&env_features) {
    return Err(format!("Version {} is is incompatible with the current environment", mc_version).into());
  }

  reporter.clear();
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
