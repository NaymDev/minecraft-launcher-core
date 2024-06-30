use super::Reporter;

pub struct EmptyReporter;

impl Reporter for EmptyReporter {
  fn setup(&self, _status: &str, _total: Option<usize>) {}
  fn progress(&self, _current: usize) {}
  fn status(&self, _status: &str) {}
  fn total(&self, _total: usize) {}
  fn done(&self) {}
}
