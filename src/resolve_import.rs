use std::error::Error;
use std::fmt;
use url::ParseError;
use url::Url;
use ModuleResolutionError::*;

/// Resolves module using this algorithm:
/// https://html.spec.whatwg.org/multipage/webappapis.html#resolve-a-module-specifier
pub fn resolve_import(
  specifier: &str,
  base: &str,
) -> Result<Url, ModuleResolutionError> {
  let url = match Url::parse(specifier) {
    // 1. Apply the URL parser to specifier.
    //    If the result is not failure, return he result.
    Ok(url) => url,

    // 2. If specifier does not start with the character U+002F SOLIDUS (/),
    //    the two-character sequence U+002E FULL STOP, U+002F SOLIDUS (./),
    //    or the three-character sequence U+002E FULL STOP, U+002E FULL STOP,
    //    U+002F SOLIDUS (../), return failure.
    Err(ParseError::RelativeUrlWithoutBase)
      if !(specifier.starts_with('/')
        || specifier.starts_with("./")
        || specifier.starts_with("../")) =>
    {
      let maybe_referrer = if base.is_empty() {
        None
      } else {
        Some(base.to_string())
      };
      return Err(ImportPrefixMissing(specifier.to_string(), maybe_referrer));
    }

    // 3. Return the result of applying the URL parser to specifier with base
    //    URL as the base URL.
    Err(ParseError::RelativeUrlWithoutBase) => {
      let base = if base == "<unknown>" {
        unreachable!()
      } else {
        Url::parse(base).map_err(InvalidBaseUrl)?
      };
      base.join(&specifier).map_err(InvalidUrl)?
    }

    // If parsing the specifier as a URL failed for a different reason than
    // it being relative, always return the original error. We don't want to
    // return `ImportPrefixMissing` or `InvalidBaseUrl` if the real
    // problem lies somewhere else.
    Err(err) => return Err(InvalidUrl(err)),
  };

  Ok(url)
}

/// Error indicating the reason resolving a module specifier failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModuleResolutionError {
  InvalidUrl(ParseError),
  InvalidBaseUrl(ParseError),
  // InvalidPath(PathBuf),
  ImportPrefixMissing(String, Option<String>),
}

impl Error for ModuleResolutionError {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    match self {
      InvalidUrl(ref err) | InvalidBaseUrl(ref err) => Some(err),
      _ => None,
    }
  }
}

impl fmt::Display for ModuleResolutionError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      InvalidUrl(ref err) => write!(f, "invalid URL: {}", err),
      InvalidBaseUrl(ref err) => {
        write!(f, "invalid base URL for relative import: {}", err)
      }
      // InvalidPath(ref path) => write!(f, "invalid module path: {:?}", path),
      ImportPrefixMissing(ref specifier, ref maybe_referrer) => write!(
        f,
        "relative import path \"{}\" not prefixed with / or ./ or ../{}",
        specifier,
        match maybe_referrer {
          Some(referrer) => format!(" Imported from \"{}\"", referrer),
          None => format!(""),
        }
      ),
    }
  }
}
