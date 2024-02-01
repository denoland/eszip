// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

mod error;
pub mod v1;
pub mod v2;

use std::pin::Pin;
use std::sync::Arc;

use deno_npm::resolution::ValidSerializedNpmResolutionSnapshot;
use futures::io::AsyncBufReadExt;
use futures::io::AsyncReadExt;
use futures::Future;
use serde::Deserialize;
use serde::Serialize;
use v2::EszipV2Modules;

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
type EszipParserFuture<R> = Pin<
  Box<
    dyn Future<Output = Result<futures::io::BufReader<R>, ParseError>> + Send,
  >,
>;

impl Eszip {
  /// Parse a byte stream into an Eszip. This function completes when the header
  /// is fully received. This does not mean that the entire file is fully
  /// received or parsed yet. To finish parsing, the future returned by this
  /// function in the second tuple slot needs to be polled.
  pub async fn parse<R: futures::io::AsyncRead + Unpin + Send + 'static>(
    reader: R,
  ) -> Result<(Eszip, EszipParserFuture<R>), ParseError> {
    let mut reader = futures::io::BufReader::new(reader);
    reader.fill_buf().await?;
    let buffer = reader.buffer();
    if EszipV2::has_magic(buffer) {
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

  /// Get the module metadata for a given module specifier. This function will
  /// follow redirects. The returned module has functions that can be used to
  /// obtain the module source and source map. The module returned from this
  /// function is guaranteed to be a valid module, which can be loaded into v8.
  ///
  /// Note that this function should be used to obtain a module; if you wish to
  /// get an import map, use [`get_import_map`](Self::get_import_map) instead.
  pub fn get_module(&self, specifier: &str) -> Option<Module> {
    match self {
      Eszip::V1(eszip) => eszip.get_module(specifier),
      Eszip::V2(eszip) => eszip.get_module(specifier),
    }
  }

  /// Get the import map for a given specifier.
  ///
  /// Note that this function should be used to obtain an import map; the returned
  /// "Module" is not necessarily a valid module that can be loaded into v8 (in
  /// other words, JSONC may be returned). If you wish to get a valid module,
  /// use [`get_module`](Self::get_module) instead.
  pub fn get_import_map(&self, specifier: &str) -> Option<Module> {
    match self {
      Eszip::V1(eszip) => eszip.get_import_map(specifier),
      Eszip::V2(eszip) => eszip.get_import_map(specifier),
    }
  }

  /// Takes the npm snapshot out of the eszip.
  pub fn take_npm_snapshot(
    &mut self,
  ) -> Option<ValidSerializedNpmResolutionSnapshot> {
    match self {
      Eszip::V1(_) => None,
      Eszip::V2(eszip) => eszip.take_npm_snapshot(),
    }
  }
}

/// Get an iterator over all the modules (including an import map, if any) in
/// this eszip archive.
///
/// Note that the iterator will iterate over the specifiers' "snapshot" of the
/// archive. If a new module is added to the archive after the iterator is
/// created via `into_iter()`, that module will not be iterated over.
impl IntoIterator for Eszip {
  type Item = (String, Module);
  type IntoIter = std::vec::IntoIter<Self::Item>;

  fn into_iter(self) -> Self::IntoIter {
    match self {
      Eszip::V1(eszip) => eszip.into_iter(),
      Eszip::V2(eszip) => eszip.into_iter(),
    }
  }
}

pub struct Module {
  pub specifier: String,
  pub kind: ModuleKind,
  inner: ModuleInner,
}

pub enum ModuleInner {
  V1(EszipV1),
  V2(EszipV2Modules),
}

impl Module {
  /// Get source code of the module.
  pub async fn source(&self) -> Option<Arc<[u8]>> {
    match &self.inner {
      ModuleInner::V1(eszip_v1) => eszip_v1.get_module_source(&self.specifier),
      ModuleInner::V2(eszip_v2) => {
        eszip_v2.get_module_source(&self.specifier).await
      }
    }
  }

  /// Take source code of the module. This will remove the source code from memory and
  /// the subsequent calls to `take_source()` will return `None`.
  /// For V1, this will take the entire module and returns the source code. We don't need
  /// to preserve module metadata for V1.
  pub async fn take_source(&self) -> Option<Arc<[u8]>> {
    match &self.inner {
      ModuleInner::V1(eszip_v1) => eszip_v1.take(&self.specifier),
      ModuleInner::V2(eszip_v2) => {
        eszip_v2.take_module_source(&self.specifier).await
      }
    }
  }

  /// Get source map of the module.
  pub async fn source_map(&self) -> Option<Arc<[u8]>> {
    match &self.inner {
      ModuleInner::V1(_) => None,
      ModuleInner::V2(eszip) => {
        eszip.get_module_source_map(&self.specifier).await
      }
    }
  }

  /// Take source map of the module. This will remove the source map from memory and
  /// the subsequent calls to `take_source_map()` will return `None`.
  pub async fn take_source_map(&self) -> Option<Arc<[u8]>> {
    match &self.inner {
      ModuleInner::V1(_) => None,
      ModuleInner::V2(eszip) => {
        eszip.take_module_source_map(&self.specifier).await
      }
    }
  }
}

/// This is the kind of module that is being stored. This is the same enum as is
/// present in [deno_core::ModuleType] except that this has additional variant
/// `Jsonc` which is used when an import map is embedded in Deno's config file
/// that can be JSONC.
/// Note that a module of type `Jsonc` can be used only as an import map, not as
/// a normal module.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleKind {
  JavaScript = 0,
  Json = 1,
  Jsonc = 2,
  OpaqueData = 3,
}

#[cfg(test)]
mod tests {
  use crate::Eszip;
  use futures::io::AllowStdIo;

  #[tokio::test]
  async fn parse_v1() {
    let file = std::fs::File::open("./src/testdata/basic.json").unwrap();
    let (eszip, fut) = Eszip::parse(AllowStdIo::new(file)).await.unwrap();
    fut.await.unwrap();
    assert!(matches!(eszip, Eszip::V1(_)));
    eszip.get_module("https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js").unwrap();
  }

  #[tokio::test]
  async fn parse_v2() {
    let file = std::fs::File::open("./src/testdata/redirect.eszip2").unwrap();
    let (eszip, fut) = Eszip::parse(AllowStdIo::new(file)).await.unwrap();
    fut.await.unwrap();
    assert!(matches!(eszip, Eszip::V2(_)));
    eszip.get_module("file:///main.ts").unwrap();
  }

  #[tokio::test]
  async fn take_source_v1() {
    let file = std::fs::File::open("./src/testdata/basic.json").unwrap();
    let (eszip, fut) = Eszip::parse(AllowStdIo::new(file)).await.unwrap();
    fut.await.unwrap();
    assert!(matches!(eszip, Eszip::V1(_)));
    let specifier = "https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js";
    let module = eszip.get_module(specifier).unwrap();
    assert_eq!(module.specifier, specifier);
    // We're taking the source from memory.
    let source = module.take_source().await.unwrap();
    assert!(!source.is_empty());
    // Source maps are not supported in v1 and should always return None.
    assert!(module.source_map().await.is_none());
    // Module shouldn't be available anymore.
    assert!(eszip.get_module(specifier).is_none());
  }

  #[tokio::test]
  async fn take_source_v2() {
    let file = std::fs::File::open("./src/testdata/redirect.eszip2").unwrap();
    let (eszip, fut) = Eszip::parse(AllowStdIo::new(file)).await.unwrap();
    fut.await.unwrap();
    assert!(matches!(eszip, Eszip::V2(_)));
    let specifier = "file:///main.ts";
    let module = eszip.get_module(specifier).unwrap();
    // We're taking the source from memory.
    let source = module.take_source().await.unwrap();
    assert!(!source.is_empty());
    let module = eszip.get_module(specifier).unwrap();
    assert_eq!(module.specifier, specifier);
    // Source shouldn't be available anymore.
    assert!(module.source().await.is_none());
    // We didn't take the source map, so it should still be available.
    assert!(module.source_map().await.is_some());
    // Now we're taking the source map.
    let source_map = module.take_source_map().await.unwrap();
    assert!(!source_map.is_empty());
    // Source map shouldn't be available anymore.
    assert!(module.source_map().await.is_none());
  }
}
