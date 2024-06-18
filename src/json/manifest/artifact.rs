use std::{ fmt::{ Debug, Display }, path::{ Path, PathBuf } };

use serde::{ Deserialize, Serialize };

#[derive(Serialize, Deserialize, Clone)]
#[serde(try_from = "String", into = "String")]
pub struct Artifact {
  original_descriptor: Option<String>,
  pub group_id: Vec<String>,
  pub artifact_id: String,
  pub version: String,
  pub classifier: Option<String>,
  pub ext: String,
}

impl Debug for Artifact {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.get_descriptor())
  }
}

impl Display for Artifact {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.get_descriptor())
  }
}

impl Artifact {
  pub fn get_file(&self) -> String {
    let mut name = format!("{}-{}", self.artifact_id, self.version);
    if let Some(classifier) = &self.classifier {
      name.push_str(&format!("-{}", classifier));
    }
    name.push_str(&format!(".{}", self.ext));
    name
  }

  pub fn get_path_vec(&self) -> Vec<String> {
    let mut vec = self.group_id.clone();
    vec.push(self.artifact_id.clone());
    vec.push(self.version.clone());
    vec.push(self.get_file());
    vec
  }

  pub fn get_path_string(&self) -> String {
    self.get_path_vec().join("/")
  }

  pub fn get_local_path(&self, root: &Path) -> PathBuf {
    let mut root = root.to_path_buf();
    for s in self.get_path_vec() {
      root = root.join(s);
    }
    root
  }

  pub fn get_descriptor(&self) -> String {
    if let Some(original_descriptor) = &self.original_descriptor {
      original_descriptor.clone()
    } else {
      let mut descriptor = format!("{}:{}:{}", self.group_id.join("."), self.artifact_id, self.version);
      if let Some(classifier) = &self.classifier {
        descriptor.push_str(&format!(":{}", classifier));
      }
      descriptor
    }
  }
}

impl TryFrom<String> for Artifact {
  type Error = String;
  fn try_from(og_value: String) -> Result<Self, Self::Error> {
    let value = og_value.clone();
    let (value, ext) = value.split_once('@').unwrap_or((&value, "jar"));

    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() < 3 {
      return Err(format!("Invalid artifact path: {}", value));
    }
    let group_id: Vec<String> = parts[0]
      .split('.')
      .map(|s| s.to_string())
      .collect();
    let artifact_id = parts[1].to_string();
    let version = parts[2].to_string();
    let classifier = parts.get(3).map(|s| s.to_string());
    Ok(Self {
      original_descriptor: Some(og_value),
      group_id,
      artifact_id,
      version,
      classifier,
      ext: ext.to_string(),
    })
  }
}

impl From<Artifact> for String {
  fn from(val: Artifact) -> Self {
    val.get_descriptor()
  }
}
