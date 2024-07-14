use std::{ env::temp_dir, fs, io::{ BufRead, BufReader }, sync::Arc };

use chrono::Utc;
use log::{ info, LevelFilter };
use minecraft_launcher_core::{
  bootstrap::{ auth::UserAuthentication, options::GameOptionsBuilder, GameBootstrap },
  java_manager::JavaRuntimeManager,
  json::{ EnvironmentFeatures, MCVersion },
  version_manager::{ downloader::progress::{ EmptyReporter, ProgressReporter }, VersionManager },
};
use reqwest::Client;
use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  SimpleLogger::new().env().with_level(LevelFilter::Debug).init()?;

  let version_id = MCVersion::new("1.21");
  let game_dir = temp_dir().join(format!(".minecraft-{}", Utc::now().timestamp_millis()));

  let client = Client::new();
  let reporter: ProgressReporter = Arc::new(EmptyReporter);
  let env_features = EnvironmentFeatures::default();

  info!("Attempting to launch the game");
  let mut version_manager = VersionManager::load(&game_dir, &env_features, Some(client.clone())).await?;

  info!("Queuing library & version downloads");
  let manifest = version_manager.resolve_local_version(&version_id, true, true).await?;
  if !manifest.applies_to_current_environment(&env_features) {
    return Err(format!("Version {} is is incompatible with the current environment", version_id).into());
  }

  info!("Installing java runtime");
  let runtime_manager = JavaRuntimeManager::load(&game_dir.join("runtimes"), &client).await?;

  let java_version = &manifest.java_version.as_ref().unwrap().component;
  runtime_manager.install_runtime(&game_dir.join("assets").join("objects"), java_version, &reporter).await?;
  let java_path = runtime_manager.get_java_executable(java_version);

  info!("Java runtime installed");

  let options = GameOptionsBuilder::default()
    .java_path(java_path)
    .natives_dir(game_dir.join("natives"))
    .game_dir(game_dir.clone())
    .authentication(UserAuthentication::offline("Player"))
    .build()?;

  info!("Downloading required files");
  version_manager.download_required_files(&manifest, &reporter, None, None).await?;

  info!("Launching game");
  let mut bootstrap = GameBootstrap::new(options);
  let mut child = bootstrap.launch_game(&manifest)?.into_inner();
  let mut stdout = BufReader::new(child.stdout.take().unwrap());
  let mut stderr = BufReader::new(child.stderr.take().unwrap());

  loop {
    let mut buf = String::new();
    if let Ok(len) = stdout.read_line(&mut buf) {
      if len > 0 {
        println!("{}", buf.trim_end());
      }
    }

    if !stderr.buffer().is_empty() {
      if let Ok(len) = stderr.read_line(&mut buf) {
        if len > 0 {
          println!("{}", buf.trim_end());
        }
      }
    }

    if let Some(status) = child.try_wait()? {
      let code = status.code().unwrap_or(0);
      info!("Game exited with code {code}");
      break;
    }
  }

  let _ = fs::remove_dir_all(&game_dir);
  Ok(())
}
