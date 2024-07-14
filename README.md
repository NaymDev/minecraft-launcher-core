# Minecraft Launcher Core

A Rust library designed to manage and launch Minecraft game versions. It handles downloading game assets, libraries, and Java runtimes, as well as managing game versions and configurations.

## Features

- Download and install Minecraft game versions
- Manage game assets and libraries
- Handle Java runtime installations
- Support for custom game directories and configurations

## Usage

The examples folder contains examples demonstrating how to use Minecraft Launcher Core. To run an example, use the following command:

```bash
cargo run --example <example_name>
```

## Features

### Version Manager

The `version_manager` feature handles downloading and managing Minecraft game versions.

```rust
use minecraft_launcher_core::{
  json::{ EnvironmentFeatures, MCVersion },
  version_manager::{ downloader::progress::{ EmptyReporter, ProgressReporter }, VersionManager },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let reporter: ProgressReporter = Arc::new(EmptyReporter);
  let env_features = EnvironmentFeatures::default();

  let version_id = MCVersion::new("1.21");
  let game_dir = PathBuf::from(var("APPDATA")?).join(".minecraft");

  let mut version_manager = VersionManager::load(&game_dir, &env_features, None).await?;
  let manifest = version_manager.resolve_local_version(&version_id, true, true).await?;

  version_manager.download_required_files(&manifest, &reporter, None, None).await?;
  Ok(())
}
```

### Java Manager

The `java_manager` feature handles downloading and installing the required Java runtime for the game.

```rust
use minecraft_launcher_core::{
  java_manager::JavaRuntimeManager,
  json::{ manifest::VersionManifest, MCVersion },
  version_manager::downloader::progress::{ EmptyReporter, ProgressReporter },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let reporter: ProgressReporter = Arc::new(EmptyReporter);

  let version_id = MCVersion::new("1.21");
  let game_dir = PathBuf::from(var("APPDATA")?).join(".minecraft");

  let manifest_file = File::open(game_dir.join(format!("versions/{0}/{0}.json", version_id)))?;
  let manifest: VersionManifest = serde_json::from_reader(manifest_file)?;

  let java_version = &manifest.java_version.unwrap().component;
  let runtime_manager = JavaRuntimeManager::load(&game_dir.join("runtimes"), &Client::new()).await?;
  runtime_manager.install_runtime(&game_dir.join("assets/objects"), java_version, &reporter).await?;
  Ok(())
}
```

### Bootstrap

The `bootstrap` feature is responsible for setting up the game environment and launching the game.

```rust
use minecraft_launcher_core::{
  bootstrap::{ auth::UserAuthentication, options::GameOptionsBuilder, GameBootstrap },
  java_manager::JavaRuntimeManager,
  json::{ manifest::VersionManifest, MCVersion },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let version_id = MCVersion::new("1.21");
  let game_dir = PathBuf::from(var("APPDATA")?).join(".minecraft");

  let manifest_file = File::open(game_dir.join(format!("versions/{0}/{0}.json", version_id)))?;
  let manifest: VersionManifest = serde_json::from_reader(manifest_file)?;

  let java_version = &manifest.java_version.as_ref().unwrap().component;
  let runtime_manager = JavaRuntimeManager::load(&game_dir.join("runtimes"), &Client::new()).await?;
  let java_path = runtime_manager.get_java_executable(java_version);

  let options = GameOptionsBuilder::default()
    .java_path(java_path)
    .natives_dir(game_dir.join("natives"))
    .game_dir(game_dir)
    .authentication(UserAuthentication::offline("Player"))
    .build()?;

  let mut bootstrap = GameBootstrap::new(options);
  bootstrap.launch_game(&manifest)?.into_inner().wait()?;
  Ok(())
}
```

### Json

The `json` feature provides the required JSON structures for the library.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the Apache-2.0 License. See the [LICENSE](LICENSE) file for details.
