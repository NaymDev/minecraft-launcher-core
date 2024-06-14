use std::{ process::{ Child, ChildStdout, ChildStderr, Command, Stdio }, io::BufReader, path::PathBuf, os::windows::process::CommandExt };

use crate::json::manifest::rule::OperatingSystem;

pub struct GameProcess {
  child: Child,
  stdout: BufReader<ChildStdout>,
  stderr: BufReader<ChildStderr>,
}

impl GameProcess {
  pub fn new(java_path: &PathBuf, game_dir: &PathBuf, args: Vec<String>) -> Self {
    let mut child = Command::new(java_path)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .current_dir(game_dir)
      .args(args)
      .creation_flags(0x08000000)
      .spawn()
      .unwrap();
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
}

pub struct GameProcessBuilder {
  arguments: Vec<String>,
  java_path: Option<PathBuf>,
  directory: Option<PathBuf>,
}

impl GameProcessBuilder {
  pub fn new() -> Self {
    Self {
      java_path: None,
      arguments: vec![],
      directory: None,
    }
  }

  pub fn with_java_path(&mut self, java_path: &PathBuf) -> &mut Self {
    self.java_path = Some(java_path.clone());
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

  pub fn directory(&mut self, directory: &PathBuf) -> &mut Self {
    self.directory = Some(directory.clone());
    self
  }

  pub fn spawn(self) -> Result<GameProcess, Box<dyn std::error::Error>> {
    let java_path = self.java_path.as_ref().ok_or("Java path not set")?;
    let directory = self.directory.as_ref().ok_or("Game directory not set")?;
    let mut args = self.get_args();
    if OperatingSystem::get_current_platform() == OperatingSystem::Windows {
      args = args
        .into_iter()
        .map(|arg| arg.replace("\"", "\\\""))
        .collect();
    }
    Ok(GameProcess::new(java_path, directory, args))
  }
}
