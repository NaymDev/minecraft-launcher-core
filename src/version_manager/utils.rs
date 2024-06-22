use std::collections::HashSet;

use async_recursion::async_recursion;

use crate::json::{ manifest::VersionManifest, MCVersion };

use super::{ error::ResolveManifestError, VersionManager };

#[async_recursion]
pub async fn resolve(
  mut version_manifest: VersionManifest,
  version_manager: &mut VersionManager,
  inheritance_trace: &mut HashSet<MCVersion>
) -> Result<VersionManifest, ResolveManifestError> {
  if let Some(inherits_from) = version_manifest.inherits_from {
    // Check for circular dependency errors
    if !inheritance_trace.insert(version_manifest.id.clone()) {
      let mut trace: Vec<String> = inheritance_trace.iter().map(ToString::to_string).collect();
      trace.reverse();
      return Err(ResolveManifestError::CircularDependency { inheritance_trace: trace, problem: version_manifest.id.clone() });
    }

    // Resolve inherited version, and download it if needed
    let local_version = if let Ok(local_version) = version_manager.get_installed_version(&inherits_from) {
      if !version_manager.is_up_to_date(&local_version).await { version_manager.install_version_by_id(&inherits_from).await? } else { local_version }
    } else {
      version_manager.install_version_by_id(&inherits_from).await?
    };

    // Recursively resolve the inherited version
    let mut resolved_manifest = resolve(local_version, version_manager, inheritance_trace).await?;
    resolved_manifest.inherits_from.take();
    resolved_manifest.id = version_manifest.id;
    resolved_manifest.updated_time = version_manifest.updated_time;
    resolved_manifest.release_time = version_manifest.release_time;
    resolved_manifest.release_type = version_manifest.release_type;

    if let Some(minecraft_arguments) = version_manifest.minecraft_arguments {
      resolved_manifest.minecraft_arguments.replace(minecraft_arguments);
    }

    if let Some(main_class) = version_manifest.main_class {
      resolved_manifest.main_class.replace(main_class);
    }

    if let Some(assets) = version_manifest.assets {
      resolved_manifest.assets.replace(assets);
    }

    if let Some(jar) = version_manifest.jar {
      resolved_manifest.jar.replace(jar);
    }

    if let Some(asset_index) = version_manifest.asset_index {
      resolved_manifest.asset_index.replace(asset_index);
    }

    if !version_manifest.libraries.is_empty() {
      let mut new_libraries = version_manifest.libraries;
      new_libraries.append(&mut resolved_manifest.libraries);
      resolved_manifest.libraries = new_libraries;
    }

    if !version_manifest.arguments.is_empty() {
      for (arg_type, mut args) in version_manifest.arguments {
        if let Some(vec) = resolved_manifest.arguments.get_mut(&arg_type) {
          vec.append(&mut args);
        } else {
          resolved_manifest.arguments.insert(arg_type, args);
        }
      }
    }

    if !version_manifest.compatibility_rules.is_empty() {
      resolved_manifest.compatibility_rules.append(&mut version_manifest.compatibility_rules);
    }

    if let Some(java_version) = version_manifest.java_version {
      resolved_manifest.java_version.replace(java_version);
    }

    Ok(resolved_manifest)
  } else {
    Ok(version_manifest)
  }
}
