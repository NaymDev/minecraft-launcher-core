use std::sync::Arc;

mod empty;
mod callback;
pub use empty::EmptyReporter;
pub use callback::{ CallbackReporter, Event };

pub type ProgressReporter = Arc<dyn Reporter + Send + Sync>;

pub trait Reporter: Send + Sync {
  /// Called when the download starts
  fn setup(&self, status: &str, total: Option<usize>);
  /// Called when reporting progress
  fn progress(&self, current: usize);
  /// Called when the status changes
  fn status(&self, status: &str);
  /// Called when the total size changes (for some reason)
  fn total(&self, total: usize);
  /// Called when the download finishes
  fn done(&self);
}
