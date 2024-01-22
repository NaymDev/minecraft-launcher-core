use std::fmt::Debug;

pub struct ProgressReporter(Box<dyn Fn(ProgressUpdate) + Send + Sync + 'static>);

impl ProgressReporter {
  pub fn new(on_update: impl Fn(ProgressUpdate) + Send + Sync + 'static) -> Self {
    Self(Box::new(on_update))
  }

  pub fn update(&self, update: ProgressUpdate) -> &Self {
    (self.0)(update);
    self
  }

  pub fn set_status(&self, status: impl AsRef<str>) -> &Self {
    self.update(ProgressUpdate::SetStatus(status.as_ref().to_string()));
    &self
  }

  pub fn set_progress(&self, progress: u32) -> &Self {
    self.update(ProgressUpdate::SetProgress(progress));
    &self
  }

  pub fn set_total(&self, total: u32) -> &Self {
    self.update(ProgressUpdate::SetTotal(total));
    &self
  }

  pub fn set(&self, status: impl AsRef<str>, progress: u32, total: u32) {
    self.update(ProgressUpdate::SetAll(status.as_ref().to_string(), progress, total));
  }

  pub fn clear(&self) {
    self.update(ProgressUpdate::Clear);
  }
}

impl Default for ProgressReporter {
  fn default() -> Self {
    Self::new(|_| {})
  }
}

impl Debug for ProgressReporter {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ProgressReporter")
  }
}

#[derive(Debug, Clone)]
pub enum ProgressUpdate {
  SetStatus(String),
  SetProgress(u32),
  SetTotal(u32),
  SetAll(String, u32, u32),
  Clear,
}
