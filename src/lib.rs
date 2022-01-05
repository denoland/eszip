mod error;
pub mod v1;
pub mod v2;

use std::borrow::Cow;

use serde::Deserialize;
use serde::Serialize;

pub use crate::error::ParseError;
pub use crate::v1::EsZipV1;
pub use crate::v2::EsZipV2;

pub use deno_graph;

pub enum EsZip {
  V1(EsZipV1),
  V2(EsZipV2),
}

impl EsZip {
  // pub fn parse(data: &[u8]) -> Result<EsZip, ParseError> {
  //   if EsZipV2::is_valid(data) {
  //     let eszip = EsZipV2::parse(data)?;
  //     Ok(EsZip::V2(eszip))
  //   } else {
  //     let eszip = EsZipV1::parse(data)?;
  //     Ok(EsZip::V1(eszip))
  //   }
  // }

  pub fn get_module(&self, specifier: &str) -> Option<Module> {
    match self {
      EsZip::V1(eszip) => eszip.get_module(specifier),
      EsZip::V2(eszip) => eszip.get_module(specifier),
    }
  }
}

pub struct Module {
  pub specifier: String,
  pub kind: ModuleKind,
  inner: ModuleInner,
}

pub enum ModuleInner {
  V1(Vec<u8>),
  V2(EsZipV2),
}

impl Module {
  pub async fn source(&self) -> Cow<'_, [u8]> {
    match &self.inner {
      ModuleInner::V1(source) => Cow::Borrowed(source),
      ModuleInner::V2(eszip) => eszip.get_module_source(&self.specifier).await,
    }
  }

  pub async fn source_map(&self) -> Option<Cow<'_, [u8]>> {
    match &self.inner {
      ModuleInner::V1(_) => None,
      ModuleInner::V2(eszip) => {
        Some(eszip.get_module_source_map(&self.specifier).await)
      }
    }
  }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleKind {
  JS = 0,
  JSON = 1,
}

// #[cfg(test)]
// mod tests {
//   use crate::EsZip;

//   #[test]
//   fn parse_v1() {
//     let data = include_bytes!("./testdata/basic.json");
//     let eszip = EsZip::parse(data).unwrap();
//     assert!(matches!(eszip, EsZip::V1(_)));
//     eszip.get_module("https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js").unwrap();
//   }

//   #[test]
//   fn parse_v2() {
//     let data = include_bytes!("./testdata/redirect.eszip2");
//     let eszip = EsZip::parse(data).unwrap();
//     assert!(matches!(eszip, EsZip::V2(_)));
//     eszip.get_module("file:///main.ts").unwrap();
//   }
// }
