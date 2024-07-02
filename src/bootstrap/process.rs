use std::{ io::BufReader, path::{ Path, PathBuf }, process::{ Child, ChildStderr, ChildStdout, Command, Stdio } };

use crate::json::manifest::rule::OperatingSystem;

use super::error::Error;

pub struct GameProcess {
  child: Child,
  stdout: BufReader<ChildStdout>,
  stderr: BufReader<ChildStderr>,
}

impl GameProcess {
  pub fn new(java_path: &PathBuf, game_dir: &PathBuf, args: Vec<String>) -> Self {
    let mut child = Command::new(java_path).stdout(Stdio::piped()).stderr(Stdio::piped()).current_dir(game_dir).args(args).spawn().unwrap();
    Self {
      stdout: BufReader::new(child.stdout.take().unwrap()),
      stderr: BufReader::new(child.stderr.take().unwrap()),
      child,
    }
  }

  pub fn inner(&self) -> &Child {
    &self.child
  }

  pub fn stdout(&mut self) -> &mut BufReader<ChildStdout> {
    &mut self.stdout
  }

  pub fn stderr(&mut self) -> &mut BufReader<ChildStderr> {
    &mut self.stderr
  }

  pub fn exit_status(&mut self) -> Option<i32> {
    let status = self.child.try_wait();
    match status {
      Ok(status) => status.and_then(|s| s.code()),
      Err(_) => Some(1),
    }
  }

  pub fn into_inner(self) -> Child {
    let mut child = self.child;
    child.stdout.replace(self.stdout.into_inner());
    child.stderr.replace(self.stderr.into_inner());
    child
  }
}

impl From<GameProcess> for Child {
  fn from(val: GameProcess) -> Self {
    val.into_inner()
  }
}

#[derive(Debug, Default)]
pub struct GameProcessBuilder {
  pub arguments: Vec<String>,
  pub java_path: Option<PathBuf>,
  pub directory: Option<PathBuf>,
}

impl GameProcessBuilder {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn with_java_path(&mut self, java_path: &Path) -> &mut Self {
    self.java_path = Some(java_path.to_path_buf());
    self
  }

  pub fn get_args(&self) -> Vec<String> {
    self.arguments.clone()
  }

  pub fn with_argument(&mut self, argument: impl AsRef<str>) -> &mut Self {
    self.arguments.push(argument.as_ref().to_string());
    self
  }

  pub fn with_arguments(&mut self, arguments: Vec<impl AsRef<str>>) -> &mut Self {
    self.arguments.extend(arguments.iter().map(|s| s.as_ref().to_string()));
    self
  }

  pub fn directory(&mut self, directory: &Path) -> &mut Self {
    self.directory = Some(directory.to_path_buf());
    self
  }

  pub fn spawn(self) -> Result<GameProcess, Error> {
    let java_path = self.java_path.as_ref().ok_or(Error::Game("Java path not set".into()))?;
    let directory = self.directory.as_ref().ok_or(Error::Game("Game directory not set".into()))?;
    let mut args = self.get_args();
    if OperatingSystem::get_current_platform() == OperatingSystem::Windows {
      args = args
        .into_iter()
        .map(|arg| arg.replace('"', "\\\""))
        .collect();
    }
    Ok(GameProcess::new(java_path, directory, args))
  }
}
