mod error;
pub mod v1;
pub mod v2;

use std::pin::Pin;
use std::sync::Arc;

use futures::Future;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use v2::ESZIP_V2_MAGIC;

pub use crate::error::FromGraphError;
pub use crate::error::ParseError;
pub use crate::v1::EszipV1;
pub use crate::v2::EszipV2;

pub use deno_ast;
pub use deno_graph;

pub enum Eszip {
  V1(EszipV1),
  V2(EszipV2),
}

/// This future needs to polled to parse the eszip file.
type EszipParserFuture<R> =
  Pin<Box<dyn Future<Output = Result<tokio::io::BufReader<R>, ParseError>>>>;

impl Eszip {
  /// Parse a byte stream into an Eszip. This function completes when the header
  /// is fully received. This does not mean that the entire file is fully
  /// received or parsed yet. To finish parsing, the future returned by this
  /// function in the second tuple slot needs to be polled.
  pub async fn parse<R: tokio::io::AsyncRead + Unpin + 'static>(
    reader: R,
  ) -> Result<(Eszip, EszipParserFuture<R>), ParseError> {
    let mut reader = tokio::io::BufReader::new(reader);
    reader.fill_buf().await?;
    let buffer = reader.buffer();
    if buffer.len() >= 8 && &buffer[..8] == ESZIP_V2_MAGIC {
      let (eszip, fut) = EszipV2::parse(reader).await?;
      Ok((Eszip::V2(eszip), Box::pin(fut)))
    } else {
      let mut buffer = Vec::new();
      reader.read_to_end(&mut buffer).await?;
      let eszip = EszipV1::parse(&buffer)?;
      let fut = async move { Ok::<_, ParseError>(reader) };
      Ok((Eszip::V1(eszip), Box::pin(fut)))
    }
  }

  pub fn get_module(&self, specifier: &str) -> Option<Module> {
    match self {
      Eszip::V1(eszip) => eszip.get_module(specifier),
      Eszip::V2(eszip) => eszip.get_module(specifier),
    }
  }
}

pub struct Module {
  pub specifier: String,
  pub kind: ModuleKind,
  inner: ModuleInner,
}

pub enum ModuleInner {
  V1(Arc<Vec<u8>>),
  V2(EszipV2),
}

impl Module {
  pub async fn source(&self) -> Arc<Vec<u8>> {
    match &self.inner {
      ModuleInner::V1(source) => source.clone(),
      ModuleInner::V2(eszip) => eszip.get_module_source(&self.specifier).await,
    }
  }

  pub async fn source_map(&self) -> Option<Arc<Vec<u8>>> {
    match &self.inner {
      ModuleInner::V1(_) => None,
      ModuleInner::V2(eszip) => {
        Some(eszip.get_module_source_map(&self.specifier).await)
      }
    }
  }
}

/// This is the kind of module that is being stored. This is the same enum as is
/// present in [deno_core], but because we can not depend on that crate, this
/// is a copy of that definition.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleKind {
  JavaScript = 0,
  Json = 1,
}

#[cfg(test)]
mod tests {
  use crate::Eszip;

  #[tokio::test]
  async fn parse_v1() {
    let file = tokio::fs::File::open("./src/testdata/basic.json")
      .await
      .unwrap();
    let (eszip, fut) = Eszip::parse(file).await.unwrap();
    fut.await.unwrap();
    assert!(matches!(eszip, Eszip::V1(_)));
    eszip.get_module("https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js").unwrap();
  }

  #[tokio::test]
  async fn parse_v2() {
    let file = tokio::fs::File::open("./src/testdata/redirect.eszip2")
      .await
      .unwrap();
    let (eszip, fut) = Eszip::parse(file).await.unwrap();
    fut.await.unwrap();
    assert!(matches!(eszip, Eszip::V2(_)));
    eszip.get_module("file:///main.ts").unwrap();
  }
}
