use super::Reporter;

pub type Callback = dyn Fn(Event) + Send + Sync + 'static;

pub struct CallbackReporter {
  callback: Box<Callback>,
}

impl CallbackReporter {
  pub fn new<T: Fn(Event) + Send + Sync + 'static>(callback: T) -> Self {
    Self { callback: Box::new(callback) }
  }
}

impl Reporter for CallbackReporter {
  fn setup(&self, status: &str, total: Option<usize>) {
    (self.callback)(Event::Setup { status: status.to_string(), total });
  }

  fn progress(&self, current: usize) {
    (self.callback)(Event::Progress(current));
  }

  fn status(&self, status: &str) {
    (self.callback)(Event::Status(status.to_string()));
  }

  fn total(&self, total: usize) {
    (self.callback)(Event::Total(total));
  }

  fn done(&self) {
    (self.callback)(Event::Done);
  }
}

pub enum Event {
  Setup {
    status: String,
    total: Option<usize>,
  },
  Progress(usize),
  Status(String),
  Total(usize),
  Done,
}
