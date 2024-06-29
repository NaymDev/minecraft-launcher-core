use thiserror::Error;

use super::downloadables::DownloadError;

#[derive(Error, Debug)]
pub enum Error {
  #[error(transparent)] DownloadError(#[from] DownloadError),
  #[error("Job '{name}' finished with {failures} failure(s)! (took {total_time}s)")] JobFailed {
    name: String,
    failures: usize,
    total_time: i64,
  },
}
