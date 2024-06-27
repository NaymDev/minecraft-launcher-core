use std::{ collections::HashMap, env::consts::ARCH, fs::{ self, create_dir_all, File }, io::{ self }, path::{ Path, PathBuf, MAIN_SEPARATOR_STR } };

use argument_substitutor::{ ArgumentSubstitutor, ArgumentSubstitutorBuilder };
use chrono::Utc;
use log::{ info, error, debug, warn };
use options::{ GameOptions, LauncherOptions, ProxyOptions };
use os_info::Type::Windows;
use process::{ GameProcess, GameProcessBuilder };
use regex::Regex;
use serde_json::json;
use zip::ZipArchive;

use crate::json::{
  manifest::{
    argument::ArgumentType,
    assets::{ AssetIndex, AssetIndexInfo, AssetObject },
    library::ExtractRules,
    rule::{ OperatingSystem, RuleFeatureType },
    VersionManifest,
  },
  EnvironmentFeatures,
  Sha1Sum,
  VersionInfo,
};

pub mod auth;
pub mod options;
pub mod process;
pub mod argument_substitutor;

mod error;
pub use error::{ Error, UnpackAssetsError, UnpackNativesError };

const DEFAULT_JRE_ARGUMENTS_32BIT: &str =
  "-Xmx2G -XX:+UnlockExperimentalVMOptions -XX:+UseG1GC -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M";
const DEFAULT_JRE_ARGUMENTS_64BIT: &str =
  "-Xmx2G -XX:+UnlockExperimentalVMOptions -XX:+UseG1GC -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M";

pub struct GameBootstrap {
  pub options: GameOptions,
  env_features: EnvironmentFeatures,
}

impl GameBootstrap {
  pub fn new(options: GameOptions) -> Self {
    let env_features = options.env_features();
    Self {
      options,
      env_features,
    }
  }

  fn get_assets_dir(&self) -> PathBuf {
    self.options.game_dir.join("assets")
  }

  fn is_win_ten(&self) -> bool {
    let os = os_info::get();
    os.os_type() == Windows && os.edition().is_some_and(|edition| edition.contains("Windows 10"))
  }
}

impl GameBootstrap {
  pub async fn launch_game(&mut self, manifest: &VersionManifest) -> Result<GameProcess, Error> {
    let os = OperatingSystem::get_current_platform();
    let game_dir = &self.options.game_dir;
    let env_features = &self.env_features;
    info!("Launching game");

    // Prepare natives
    if let Err(err) = self.unpack_natives(manifest) {
      error!("Couldn't unpack natives! {err}");
      return Err(Error::UnpackNatives(err));
    }

    let game_assets_dir = self.reconstruct_assets(manifest).map_err(|err| {
      error!("Couldn't unpack assets! {err}");
      Error::UnpackAssets(err)
    })?;

    // Prepare game directory
    info!("Launching in {}", game_dir.display());
    if !game_dir.exists() {
      if create_dir_all(game_dir).is_err() {
        error!("Aborting launch; couldn't create game directory");
        return Err(Error::Launch("couldn't create game directory"));
      }
    } else if !game_dir.is_dir() {
      error!("Aborting launch; game directory is not actually a directory");
      return Err(Error::Launch("game directory is not actually a directory"));
    }

    // Extra: Prepare server resource packs directory
    let server_resource_packs_dir = game_dir.join("server-resource-packs");
    create_dir_all(server_resource_packs_dir)?;

    let mut game_process_builder = GameProcessBuilder::new();
    game_process_builder.with_java_path(&self.options.java_path);
    game_process_builder.directory(game_dir);

    if let Some(jvm_args) = &self.options.jvm_args {
      game_process_builder.with_arguments(jvm_args.iter().collect());
    } else {
      let args = if ARCH == "x86_64" { DEFAULT_JRE_ARGUMENTS_64BIT } else { DEFAULT_JRE_ARGUMENTS_32BIT };
      game_process_builder.with_arguments(args.split(' ').collect());
    }

    let substitutor = self.create_arguments_substitutor(manifest, &game_assets_dir)?;

    // Add JVM args
    if !manifest.arguments.is_empty() {
      if let Some(jvm_arguments) = manifest.arguments.get(&ArgumentType::Jvm) {
        game_process_builder.with_arguments(
          jvm_arguments
            .iter()
            .filter_map(|v| v.apply(env_features))
            .flatten()
            .map(|arg| substitutor.substitute(arg))
            .collect()
        );
      }
    } else if manifest.minecraft_arguments.is_some() {
      // Manifest uses old format
      if os == OperatingSystem::Windows {
        game_process_builder.with_argument("-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump");
        if self.is_win_ten() {
          game_process_builder.with_arguments(vec!["-Dos.name=Windows 10", "-Dos.version=10.0"]);
        }
      } else if os == OperatingSystem::Osx {
        game_process_builder.with_arguments(substitutor.substitute_all(vec!["-Xdock:icon=${asset=icons/minecraft.icns}", "-Xdock:name=Minecraft"]));
      }

      game_process_builder.with_arguments(
        substitutor.substitute_all(
          vec![
            "-Djava.library.path=${natives_directory}",
            "-Dminecraft.launcher.brand=${launcher_name}",
            "-Dminecraft.launcher.version=${launcher_version}",
            "-Dminecraft.client.jar=${primary_jar}",
            "-cp ${classpath}"
          ]
        )
      );
    }

    game_process_builder.with_argument(manifest.get_main_class());
    info!("Half command: {}", game_process_builder.get_args().join(" "));
    if !manifest.arguments.is_empty() {
      if let Some(arguments) = manifest.arguments.get(&ArgumentType::Game) {
        game_process_builder.with_arguments(
          arguments
            .iter()
            .filter_map(|v| v.apply(env_features))
            .flatten()
            .map(|arg| substitutor.substitute(arg))
            .collect()
        );
      }
    } else if let Some(minecraft_arguments) = &manifest.minecraft_arguments {
      game_process_builder.with_arguments(
        minecraft_arguments
          .split(' ')
          .map(|arg| substitutor.substitute(arg))
          .collect()
      );

      if env_features.has_feature(&RuleFeatureType::IsDemoUser, &json!(true)) {
        game_process_builder.with_argument("--demo");
      }

      if env_features.has_feature(&RuleFeatureType::HasCustomResolution, &json!(true)) {
        game_process_builder.with_arguments(vec!["--width", &substitutor.substitute("${resolution_width}")]);
        game_process_builder.with_arguments(vec!["--height", &substitutor.substitute("${resolution_height}")]);
      }
    }

    if let ProxyOptions::Proxy { host, port, username, password } = &self.options.proxy {
      game_process_builder.with_arguments(vec!["--proxyHost", host, "--proxyPort", &port.to_string()]);

      if let Some(username) = username {
        game_process_builder.with_arguments(vec!["--proxyUser", username]);
      }

      if let Some(password) = password {
        game_process_builder.with_arguments(vec!["--proxyPass", password]);
      }
    }

    // Print args for debug purposes
    {
      // Remove token from args
      let args_vec = game_process_builder.get_args();
      let mut args = args_vec.join(" ");
      if let Some(token) = &self.options.authentication.access_token {
        args = args.replace(token, "?????");
      }
      debug!("Running {} {}", &self.options.java_path.display(), args);

      let regex = Regex::new(r"\$\{.+\}")?;
      args_vec
        .iter()
        .filter_map(|arg| regex.find(arg))
        .for_each(|arg| debug!("Unresolved variable - {:?}", arg.as_str()));
    }

    game_process_builder.spawn()
  }

  fn unpack_natives(&self, manifest: &VersionManifest) -> Result<(), UnpackNativesError> {
    let os = OperatingSystem::get_current_platform();
    let natives_dir = &self.options.natives_dir;
    info!("Unpacking natives to {}", natives_dir.display());
    create_dir_all(natives_dir).map_err(UnpackNativesError::CreateNativesFolder)?;

    let libs = manifest.get_relevant_libraries(&self.env_features);

    fn unpack_native(natives_dir: &Path, mut zip_archive: ZipArchive<File>, extract_rules: Option<&ExtractRules>) -> Result<(), UnpackNativesError> {
      for i in 0..zip_archive.len() {
        let mut file = zip_archive.by_index(i)?;
        let file_zip_path = file.enclosed_name().unwrap().to_owned();
        if let Some(extract_rules) = extract_rules {
          if !extract_rules.should_extract(&file_zip_path) {
            continue;
          }
        }

        let output_file = natives_dir.join(file_zip_path);
        if let Some(parent) = output_file.parent() {
          create_dir_all(parent).map_err(UnpackNativesError::UnpackNative)?;
        }
        if !file.is_dir() {
          let mut output_file = File::create(output_file).map_err(UnpackNativesError::UnpackNative)?;
          io::copy(&mut file, &mut output_file).map_err(UnpackNativesError::UnpackNative)?;
        }
      }
      Ok(())
    }

    for lib in libs {
      let natives = &lib.natives;
      if let Some(native_id) = natives.get(&os) {
        let file = &self.options.game_dir.join("libraries").join(lib.get_artifact_path(Some(native_id)).replace('/', MAIN_SEPARATOR_STR));
        let file = File::open(file).map_err(UnpackNativesError::ReadNative)?;
        let zip_file = ZipArchive::new(file)?;
        let extract_rules = lib.extract.as_ref();
        let _ = unpack_native(natives_dir, zip_file, extract_rules); // Ignore errors
      }
    }

    Ok(())
  }

  /// Reconstructs the assets based on the provided version manifest.
  ///
  /// This function attempts to reconstruct the assets by reading the asset index
  /// from the version manifest and copying the necessary files from the `objects` directory
  /// to either a `virtual` directory or directly to the `resources` directory if specified by
  /// the asset index.
  /// If no asset index file is found in the assets directory, it will silently fail.
  ///
  /// # Arguments
  /// * `manifest` - A reference to the `VersionManifest` which contains information
  ///   about the asset index and other version details.
  ///
  /// # Returns
  /// A `Result` which is:
  /// - `Ok(PathBuf)` containing the path to the reconstructed assets directory.
  /// - `Err(UnpackAssetsError)` describing the error encountered during the operation.
  ///
  /// # Errors
  /// This function can return an `UnpackAssetsError` in several cases, including:
  /// - No asset index is found in the provided manifest (`NoAssetIndex`).
  /// - Failure to open or parse the asset index file (`OpenAssetIndex`, `ParseAssetIndex`).
  /// - Errors reading asset objects or unpacking them into the target directory (`ReadAssetObject`, `UnpackAssetObject`).
  fn reconstruct_assets(&self, manifest: &VersionManifest) -> Result<PathBuf, UnpackAssetsError> {
    let asset_index_info = manifest.asset_index.as_ref().ok_or(UnpackAssetsError::NoAssetIndex)?;

    let assets_dir = self.options.game_dir.join("assets");
    let indexes_dir = assets_dir.join("indexes");
    let objects_dir = assets_dir.join("objects");

    let asset_index_id = &asset_index_info.id;
    let asset_index_file = indexes_dir.join(format!("{}.json", asset_index_id));
    let mut virtual_dir = assets_dir.join("virtual").join(asset_index_id);

    // Open asset index file
    if !asset_index_file.is_file() {
      warn!("No assets index file {}; can't reconstruct assets", virtual_dir.display());
      return Ok(virtual_dir); // Should throw also?
    } else {
      let asset_index = self.get_asset_index(asset_index_info).map_err(UnpackAssetsError::ParseAssetIndex)?;
      if asset_index.map_to_resources {
        virtual_dir = self.options.game_dir.join("resources");
      }

      if asset_index.is_virtual || asset_index.map_to_resources {
        info!("Reconstructing virtual assets folder at {}", virtual_dir.display());

        for (asset_file_name, AssetObject { hash: asset_hash, .. }) in asset_index.get_file_map() {
          let asset_hash_string = asset_hash.to_string();
          let asset_file = virtual_dir.join(asset_file_name);
          let object_file = objects_dir.join(&asset_hash_string[0..2]).join(asset_hash_string);

          let mut should_copy = true;
          if asset_file.is_file() {
            let mut file = File::open(&asset_file).map_err(UnpackAssetsError::ReadAssetObject)?;
            let hash = Sha1Sum::from_reader(&mut file)?;
            if hash != *asset_hash {
              should_copy = true;
            }
          }

          if should_copy {
            info!("Copying asset for virtual or resource-mapped: {}", asset_file.display());
            fs::copy(object_file, asset_file).map_err(UnpackAssetsError::UnpackAssetObject)?;
          }
        }

        let _ = fs::write(virtual_dir.join(".lastused"), Utc::now().to_rfc3339().as_bytes());
      }
    }

    Ok(virtual_dir)
  }

  fn create_arguments_substitutor(&self, manifest: &VersionManifest, game_assets_dir: &Path) -> Result<ArgumentSubstitutor, Error> {
    let asset_index_info = manifest.asset_index.as_ref();
    let mut substitutor = ArgumentSubstitutorBuilder::new();

    let classpath_separator = if OperatingSystem::get_current_platform() == OperatingSystem::Windows { ";" } else { ":" };
    let version_id = manifest.id.to_string();
    let game_dir = &self.options.game_dir;

    let classpath = self.construct_classpath(manifest)?;
    let assets_dir = self.get_assets_dir();
    let libraries_dir = game_dir.join("libraries");
    let natives_dir = &self.options.natives_dir;

    let launcher_opts = self.options.launcher_options.as_ref();

    let jar_id = manifest.get_jar().to_string();
    let jar_path = game_dir.join("versions").join(&jar_id).join(format!("{}.jar", &jar_id));

    let asset_index_substitutions = {
      let mut map = HashMap::new();

      if let Some(asset_index) = asset_index_info.and_then(|info| self.get_asset_index(info).ok()) {
        let objects_dir = assets_dir.join("objects");
        for (asset_name, AssetObject { hash, .. }) in asset_index.get_file_map() {
          let hash = hash.to_string();
          let asset_path = objects_dir.join(&hash[0..2]).join(hash);
          if let Some(asset_path) = asset_path.to_str() {
            map.insert(format!("asset={asset_name}"), asset_path.to_string());
          }
        }
      }

      map
    };

    let auth = &self.options.authentication;
    substitutor
      .add("auth_access_token", auth.access_token())
      .add("auth_session", auth.auth_session())

      .add("auth_player_name", &auth.username)
      .add("auth_uuid", auth.uuid.to_string())
      .add("user_type", auth.user_type());

    substitutor
      .add("profile_name", "")
      .add("version_name", &version_id)
      .add("game_directory", game_dir.to_str().unwrap_or_default())
      .add("game_assets", game_assets_dir.to_str().unwrap_or_default())
      .add("assets_root", assets_dir.to_str().unwrap_or_default())
      .add("assets_index_name", asset_index_info.map(|info| info.id.as_str()).unwrap_or_default())
      .add("version_type", manifest.get_type().get_name());

    if let Some(resolution) = &self.options.resolution {
      substitutor.add("resolution_width", resolution.width().to_string());
      substitutor.add("resolution_height", resolution.height().to_string());
    } else {
      substitutor.add("resolution_width", "");
      substitutor.add("resolution_height", "");
    }

    substitutor.add("language", "en-us").add_all(asset_index_substitutions);

    if let Some(LauncherOptions { launcher_name, launcher_version }) = launcher_opts {
      substitutor.add("launcher_name", launcher_name);
      substitutor.add("launcher_version", launcher_version);
    } else {
      substitutor.add("launcher_name", "").add("launcher_version", "");
    }

    substitutor
      .add("natives_directory", natives_dir.to_str().unwrap_or_default())

      .add("classpath", &classpath)
      .add("classpath_separator", classpath_separator)
      .add("primary_jar", jar_path.to_str().unwrap_or_default());

    substitutor.add("clientid", ""); // TODO: figure out
    substitutor.add("auth_xuid", auth.xuid().unwrap_or_default());

    substitutor.add("library_directory", libraries_dir.to_str().unwrap_or_default()); // Forge compatibility

    // substitutor.add_all(self.options.authentication.get_extra_substitutors());
    substitutor.add_all(self.options.substitutor_overrides.clone()); // Override if needed

    Ok(substitutor.build())
  }

  fn construct_classpath(&self, manifest: &VersionManifest) -> Result<String, Error> {
    let os = OperatingSystem::get_current_platform();
    let separator = if os == OperatingSystem::Windows { ";" } else { ":" };
    let classpath = manifest.get_classpath(&os, &self.options.game_dir, &self.env_features);

    let mut vec = vec![];
    for path in &classpath {
      if !path.is_file() {
        return Err(Error::ClasspathFileNotFound(path.to_path_buf()));
      }
      if let Some(path) = path.to_str() {
        vec.push(path.to_string());
      } else {
        return Err(Error::InvalidClasspathPath(path.to_path_buf()));
      }
    }
    Ok(vec.join(separator))
  }

  fn get_asset_index(&self, asset_index_info: &AssetIndexInfo) -> Result<AssetIndex, Box<dyn std::error::Error>> {
    let index_id = &asset_index_info.id;
    let index_file = self.get_assets_dir().join("indexes").join(format!("{}.json", index_id));

    let file = &mut File::open(index_file)?;
    Ok(serde_json::from_reader(file)?)
  }
}
