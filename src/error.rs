use thiserror::Error;

use crate::parser::ParseError;
use crate::resolve_import::ModuleResolutionError;

#[derive(Error, Debug)]
pub enum Error {
  #[error("module with specifier '{specifier}' not found")]
  NotFound { specifier: String },
  #[error(transparent)]
  Parse(#[from] ParseError),
  #[error(transparent)]
  ModuleResolution(#[from] ModuleResolutionError),
  #[error(
    "invalid redirect for '{specifier}': missing or invalid Location header"
  )]
  InvalidRedirect { specifier: String },
  #[error("failed to fetch '{specifier}': {inner}")]
  Download {
    specifier: String,
    inner: reqwest::Error,
  },
  #[error(transparent)]
  Other(Box<dyn std::error::Error + Send + 'static>),
}

pub fn reqwest_error(specifier: String, error: reqwest::Error) -> Error {
  if error.is_connect()
    || error.is_decode()
    || error.is_status()
    || error.is_timeout()
  {
    Error::Download {
      specifier,
      inner: error,
    }
  } else {
    Error::Other(Box::new(error))
  }
}
