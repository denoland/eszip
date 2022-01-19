use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Poll;
use std::task::Waker;

use deno_ast::EmitOptions;
use deno_ast::TranspiledSource;
use deno_graph::ModuleGraph;
use futures::future::poll_fn;
use sha2::Digest;
use sha2::Sha256;
use tokio::io::AsyncReadExt;

use crate::error::ParseError;
use crate::Module;
use crate::ModuleInner;
use crate::ModuleKind;

const ESZIP_V2_MAGIC: &[u8; 8] = b"ESZIP_V2";

#[derive(Debug, PartialEq)]
#[repr(u8)]
enum HeaderFrameKind {
  Module = 0,
  Redirect = 1,
}

#[derive(Debug, Default)]
pub struct EsZipV2 {
  modules: Arc<Mutex<HashMap<String, EszipV2Module>>>,
}

#[derive(Debug)]
enum EszipV2Module {
  Module {
    kind: ModuleKind,
    source: EszipV2SourceSlot,
    source_map: EszipV2SourceSlot,
  },
  Redirect {
    target: String,
  },
}

#[derive(Debug)]
enum EszipV2SourceSlot {
  Pending {
    offset: usize,
    length: usize,
    wakers: Vec<Waker>,
  },
  Ready(Vec<u8>),
}

impl EszipV2SourceSlot {
  fn bytes(&self) -> &[u8] {
    match self {
      EszipV2SourceSlot::Ready(v) => v,
      _ => panic!("EszipV2SourceSlot::unwrap() called on a pending slot"),
    }
  }
}

impl EsZipV2 {
  pub async fn parse<R: tokio::io::AsyncRead + Unpin>(
    mut reader: tokio::io::BufReader<R>,
  ) -> Result<(EsZipV2, impl Future<Output = Result<(), ParseError>>), ParseError>
  {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic).await?;

    if magic != *ESZIP_V2_MAGIC {
      return Err(ParseError::InvalidV2);
    }

    let header_len = reader.read_u32().await? as usize;
    let mut header_and_hash = vec![0u8; header_len + 32];
    reader.read_exact(&mut header_and_hash).await?;

    let header_bytes = &header_and_hash[..header_len];

    let mut hasher = Sha256::new();
    hasher.update(&header_bytes);
    let actual_hash = hasher.finalize();
    let expected_hash = &header_and_hash[header_bytes.len()..];
    if &*actual_hash != expected_hash {
      return Err(ParseError::InvalidV2HeaderHash);
    }

    let mut modules: HashMap<String, EszipV2Module> = HashMap::new();

    let mut read = 0;

    macro_rules! read {
      ($n:expr, $err:expr) => {{
        if read + $n > header_len {
          return Err(ParseError::InvalidV2Header($err));
        }
        let start = read;
        read += $n;
        &header_bytes[start..read]
      }};
    }

    while read < header_len {
      let specifier_len =
        u32::from_be_bytes(read!(4, "specifier len").try_into().unwrap())
          as usize;
      let specifier =
        String::from_utf8(read!(specifier_len, "specifier").to_vec())
          .map_err(|_| ParseError::InvalidV2Specifier(read))?;

      let entry_kind = read!(1, "entry kind")[0];
      match entry_kind {
        0 => {
          let source_offset =
            u32::from_be_bytes(read!(4, "source offset").try_into().unwrap());
          let source_len =
            u32::from_be_bytes(read!(4, "source len").try_into().unwrap());
          let source_map_offset = u32::from_be_bytes(
            read!(4, "source map offset").try_into().unwrap(),
          );
          let source_map_len =
            u32::from_be_bytes(read!(4, "source map len").try_into().unwrap());
          let kind = match read!(1, "module kind")[0] {
            0 => ModuleKind::JS,
            1 => ModuleKind::JSON,
            n => return Err(ParseError::InvalidV2ModuleKind(n, read)),
          };
          let module = EszipV2Module::Module {
            kind,
            source: EszipV2SourceSlot::Pending {
              offset: source_offset as usize,
              length: source_len as usize,
              wakers: Vec::new(),
            },
            source_map: EszipV2SourceSlot::Pending {
              offset: source_map_offset as usize,
              length: source_map_len as usize,
              wakers: Vec::new(),
            },
          };
          modules.insert(specifier, module);
        }
        1 => {
          let target_len =
            u32::from_be_bytes(read!(4, "target len").try_into().unwrap())
              as usize;
          let target = String::from_utf8(read!(target_len, "target").to_vec())
            .map_err(|_| ParseError::InvalidV2Specifier(read))?;
          modules.insert(specifier, EszipV2Module::Redirect { target });
        }
        n => return Err(ParseError::InvalidV2EntryKind(n, read)),
      };
    }

    let mut source_offsets = modules
      .iter()
      .filter_map(|(specifier, m)| {
        if let EszipV2Module::Module {
          source: EszipV2SourceSlot::Pending { offset, length, .. },
          ..
        } = m
        {
          Some((*offset, (*length, specifier.clone())))
        } else {
          None
        }
      })
      .collect::<HashMap<_, _>>();

    let mut source_map_offsets = modules
      .iter()
      .filter_map(|(specifier, m)| {
        if let EszipV2Module::Module {
          source_map: EszipV2SourceSlot::Pending { offset, length, .. },
          ..
        } = m
        {
          Some((*offset, (*length, specifier.clone())))
        } else {
          None
        }
      })
      .collect::<HashMap<_, _>>();

    let modules = Arc::new(Mutex::new(modules));
    let modules_ = modules.clone();

    let fut = async move {
      let modules = modules_;

      let sources_len = reader.read_u32().await? as usize;
      let mut read = 0;

      while read < sources_len {
        let (length, specifier) = source_offsets
          .remove(&read)
          .ok_or(ParseError::InvalidV2SourceOffset(read))?;

        let mut source_bytes = vec![0u8; length];
        reader.read_exact(&mut source_bytes).await?;

        let expected_hash = &mut [0u8; 32];
        reader.read_exact(expected_hash).await?;

        let mut hasher = Sha256::new();
        hasher.update(&source_bytes);
        let actual_hash = hasher.finalize();
        if &*actual_hash != expected_hash {
          return Err(ParseError::InvalidV2SourceHash(specifier));
        }

        read += length + 32;

        let wakers = {
          let mut modules = modules.lock().unwrap();
          let module = modules.get_mut(&specifier).expect("module not found");
          if let EszipV2Module::Module { source, .. } = module {
            let slot =
              std::mem::replace(source, EszipV2SourceSlot::Ready(source_bytes));

            if let EszipV2SourceSlot::Pending { wakers, .. } = slot {
              wakers
            } else {
              panic!("already populated source slot");
            }
          } else {
            panic!("invalid module type");
          }
        };
        for w in wakers {
          w.wake();
        }
      }

      let source_maps_len = reader.read_u32().await? as usize;
      let mut read = 0;

      while read < source_maps_len {
        let (length, specifier) = source_map_offsets
          .remove(&read)
          .ok_or(ParseError::InvalidV2SourceOffset(read))?;

        let mut source_map_bytes = vec![0u8; length];
        reader.read_exact(&mut source_map_bytes).await?;

        let expected_hash = &mut [0u8; 32];
        reader.read_exact(expected_hash).await?;

        let mut hasher = Sha256::new();
        hasher.update(&source_map_bytes);
        let actual_hash = hasher.finalize();
        if &*actual_hash != expected_hash {
          return Err(ParseError::InvalidV2SourceHash(specifier));
        }

        read += length + 32;

        let wakers = {
          let mut modules = modules.lock().unwrap();
          let module = modules.get_mut(&specifier).expect("module not found");
          if let EszipV2Module::Module { source_map, .. } = module {
            let slot = std::mem::replace(
              source_map,
              EszipV2SourceSlot::Ready(source_map_bytes),
            );

            if let EszipV2SourceSlot::Pending { wakers, .. } = slot {
              wakers
            } else {
              panic!("already populated source_map slot");
            }
          } else {
            panic!("invalid module type");
          }
        };
        for w in wakers {
          w.wake();
        }
      }

      Ok(())
    };

    Ok((EsZipV2 { modules }, fut))
  }

  pub fn into_bytes(self) -> Vec<u8> {
    let mut header: Vec<u8> = ESZIP_V2_MAGIC.to_vec();
    header.extend_from_slice(&[0u8; 4]); // add 4 bytes of space to put the header length in later
    let mut sources: Vec<u8> = Vec::new();
    let mut source_maps: Vec<u8> = Vec::new();

    let modules = self.modules.lock().unwrap();

    for (specifier, module) in &*modules {
      let specifier_bytes = specifier.as_bytes();
      let specifier_length = specifier_bytes.len() as u32;
      header.extend_from_slice(&specifier_length.to_be_bytes());
      header.extend_from_slice(specifier_bytes);

      match module {
        EszipV2Module::Module {
          kind,
          source,
          source_map,
        } => {
          header.push(HeaderFrameKind::Module as u8);

          // add the source to the `sources` bytes
          let source_bytes = source.bytes();
          let source_length = source_bytes.len() as u32;
          let source_offset = sources.len() as u32;
          sources.extend_from_slice(source_bytes);
          let mut hasher = Sha256::new();
          hasher.update(source_bytes);
          let source_hash = hasher.finalize();
          sources.extend_from_slice(&source_hash);

          header.extend_from_slice(&source_offset.to_be_bytes());
          header.extend_from_slice(&source_length.to_be_bytes());

          // add the source map to the `source_maps` bytes
          let source_map_bytes = source_map.bytes();
          let source_map_length = source_map_bytes.len() as u32;
          let source_map_offset = source_maps.len() as u32;
          source_maps.extend_from_slice(source_map_bytes);
          let mut hasher = Sha256::new();
          hasher.update(source_map_bytes);
          let source_map_hash = hasher.finalize();
          source_maps.extend_from_slice(&source_map_hash);

          header.extend_from_slice(&source_map_offset.to_be_bytes());
          header.extend_from_slice(&source_map_length.to_be_bytes());

          // add module kind to the header
          header.push(*kind as u8);
        }
        EszipV2Module::Redirect { target } => {
          header.push(HeaderFrameKind::Redirect as u8);
          let target_bytes = target.as_bytes();
          let target_length = target_bytes.len() as u32;
          header.extend_from_slice(&target_length.to_be_bytes());
          header.extend_from_slice(target_bytes);
        }
      }
    }

    // populate header length
    let header_length =
      (header.len() - ESZIP_V2_MAGIC.len() - size_of::<u32>()) as u32;
    header[ESZIP_V2_MAGIC.len()..ESZIP_V2_MAGIC.len() + size_of::<u32>()]
      .copy_from_slice(&header_length.to_be_bytes());

    // add header hash
    let header_bytes = &header[ESZIP_V2_MAGIC.len() + size_of::<u32>()..];
    let mut hasher = sha2::Sha256::new();
    hasher.update(&header_bytes);
    let header_hash = hasher.finalize();
    header.extend_from_slice(&header_hash);

    let mut bytes = header;

    let sources_len = sources.len() as u32;
    bytes.extend_from_slice(&sources_len.to_be_bytes());
    bytes.extend_from_slice(&sources);

    let source_maps_len = source_maps.len() as u32;
    bytes.extend_from_slice(&source_maps_len.to_be_bytes());
    bytes.extend_from_slice(&source_maps);

    bytes
  }

  pub fn from_graph(graph: ModuleGraph) -> Result<Self, anyhow::Error> {
    let emit_options = EmitOptions {
      inline_sources: true,
      inline_source_map: false,
      source_map: true,
      ..Default::default()
    };

    let mut modules = HashMap::new();

    for module in graph.modules() {
      let TranspiledSource {
        text: source,
        source_map: maybe_source_map,
      } = module.parsed_source.transpile(&emit_options)?;
      let source_map = maybe_source_map.unwrap_or_default();
      let specifier = module.specifier.to_string();

      let module = EszipV2Module::Module {
        kind: ModuleKind::JS,
        source: EszipV2SourceSlot::Ready(source.into_bytes()),
        source_map: EszipV2SourceSlot::Ready(source_map.into_bytes()),
      };
      modules.insert(specifier, module);
    }

    for module in graph.synthetic_modules() {
      if module.media_type == deno_graph::MediaType::Json {
        let source = module.maybe_source.as_ref().unwrap();
        let specifier = module.specifier.to_string();
        let module = EszipV2Module::Module {
          kind: ModuleKind::JSON,
          source: EszipV2SourceSlot::Ready(source.as_bytes().to_owned()),
          source_map: EszipV2SourceSlot::Ready(vec![]),
        };
        modules.insert(specifier, module);
      }
    }

    for (specifier, target) in graph.redirects {
      let module = EszipV2Module::Redirect {
        target: target.to_string(),
      };
      modules.insert(specifier.to_string(), module);
    }

    Ok(Self {
      modules: Arc::new(Mutex::new(modules)),
    })
  }

  pub fn get_module(&self, specifier: &str) -> Option<Module> {
    let mut specifier = specifier;
    let mut visited = HashSet::new();
    let modules = self.modules.lock().unwrap();
    loop {
      visited.insert(specifier);
      let module = modules.get(specifier)?;
      match module {
        EszipV2Module::Module { kind, .. } => {
          return Some(Module {
            specifier: specifier.to_string(),
            kind: *kind,
            inner: ModuleInner::V2(EsZipV2 {
              modules: self.modules.clone(),
            }),
          });
        }
        EszipV2Module::Redirect { target } => {
          specifier = target;
          if visited.contains(specifier) {
            return None;
          }
        }
      }
    }
  }

  pub(crate) async fn get_module_source<'a>(
    &'a self,
    specifier: &str,
  ) -> Cow<'a, [u8]> {
    poll_fn(|cx| {
      let mut modules = self.modules.lock().unwrap();
      let module = modules.get_mut(specifier).unwrap();
      let slot = match module {
        EszipV2Module::Module { source, .. } => source,
        EszipV2Module::Redirect { .. } => {
          panic!("redirects are already resolved")
        }
      };
      match slot {
        EszipV2SourceSlot::Pending { wakers, .. } => {
          wakers.push(cx.waker().clone());
          Poll::Pending
        }
        EszipV2SourceSlot::Ready(bytes) => {
          Poll::Ready(Cow::Owned(bytes.clone()))
        }
      }
    })
    .await
  }

  pub(crate) async fn get_module_source_map<'a>(
    &'a self,
    specifier: &str,
  ) -> Cow<'a, [u8]> {
    poll_fn(|cx| {
      let mut modules = self.modules.lock().unwrap();
      let module = modules.get_mut(specifier).unwrap();
      let slot = match module {
        EszipV2Module::Module { source_map, .. } => source_map,
        EszipV2Module::Redirect { .. } => {
          panic!("redirects are already resolved")
        }
      };
      match slot {
        EszipV2SourceSlot::Pending { wakers, .. } => {
          wakers.push(cx.waker().clone());
          Poll::Pending
        }
        EszipV2SourceSlot::Ready(bytes) => {
          Poll::Ready(Cow::Owned(bytes.clone()))
        }
      }
    })
    .await
  }

  pub fn specifiers(&self) -> Vec<String> {
    let modules = self.modules.lock().unwrap();
    modules.keys().cloned().collect()
  }
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;
  use std::path::Path;
  use std::sync::Arc;

  use deno_graph::source::LoadResponse;
  use deno_graph::ModuleSpecifier;
  use tokio::io::BufReader;
  use url::Url;

  use crate::ModuleKind;

  struct FileLoader;

  impl deno_graph::source::Loader for FileLoader {
    fn load(
      &mut self,
      specifier: &ModuleSpecifier,
      is_dynamic: bool,
    ) -> deno_graph::source::LoadFuture {
      assert!(!is_dynamic);
      assert_eq!(specifier.scheme(), "file");
      let path = format!("./src/testdata/source{}", specifier.path());
      Box::pin(async move {
        let path = Path::new(&path);
        let resolved = path.canonicalize().unwrap();
        let source = tokio::fs::read_to_string(&resolved).await.unwrap();
        let specifier =
          resolved.file_name().unwrap().to_string_lossy().to_string();
        let specifier = Url::parse(&format!("file:///{}", specifier)).unwrap();
        Ok(Some(LoadResponse {
          content: Arc::new(source),
          maybe_headers: None,
          specifier,
        }))
      })
    }
  }

  #[tokio::test]
  async fn from_graph_redirect() {
    let roots = vec![ModuleSpecifier::parse("file:///main.ts").unwrap()];
    let graph = deno_graph::create_graph(
      roots,
      false,
      None,
      &mut FileLoader,
      None,
      None,
      None,
      None,
    )
    .await;
    graph.valid().unwrap();
    let eszip = super::EsZipV2::from_graph(graph).unwrap();
    let module = eszip.get_module("file:///main.ts").unwrap();
    assert_eq!(module.specifier, "file:///main.ts");
    let source = module.source().await;
    assert_eq!(&*source, include_bytes!("./testdata/emit/main.ts"));
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, include_bytes!("./testdata/emit/main.ts.map"));
    assert_eq!(module.kind, ModuleKind::JS);
    let module = eszip.get_module("file:///a.ts").unwrap();
    assert_eq!(module.specifier, "file:///b.ts");
    let source = module.source().await;
    assert_eq!(&*source, include_bytes!("./testdata/emit/b.ts"));
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, include_bytes!("./testdata/emit/b.ts.map"));
    assert_eq!(module.kind, ModuleKind::JS);
  }

  #[tokio::test]
  async fn from_graph_json() {
    let roots = vec![ModuleSpecifier::parse("file:///json.ts").unwrap()];
    let graph = deno_graph::create_graph(
      roots,
      false,
      None,
      &mut FileLoader,
      None,
      None,
      None,
      None,
    )
    .await;
    graph.valid().unwrap();
    let eszip = super::EsZipV2::from_graph(graph).unwrap();
    let module = eszip.get_module("file:///json.ts").unwrap();
    assert_eq!(module.specifier, "file:///json.ts");
    let source = module.source().await;
    assert_eq!(&*source, include_bytes!("./testdata/emit/json.ts"));
    let _source_map = module.source_map().await.unwrap();
    assert_eq!(module.kind, ModuleKind::JS);
    let module = eszip.get_module("file:///data.json").unwrap();
    assert_eq!(module.specifier, "file:///data.json");
    let source = module.source().await;
    assert_eq!(&*source, include_bytes!("./testdata/emit/data.json"));
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, &[0; 0]);
    assert_eq!(module.kind, ModuleKind::JSON);
  }

  #[tokio::test]
  async fn file_format_parse() {
    let file = tokio::fs::File::open("./src/testdata/redirect.eszip2")
      .await
      .unwrap();
    let (eszip, fut) =
      super::EsZipV2::parse(BufReader::new(file)).await.unwrap();

    let test = async move {
      let module = eszip.get_module("file:///main.ts").unwrap();
      assert_eq!(module.specifier, "file:///main.ts");
      let source = module.source().await;
      assert_eq!(&*source, include_bytes!("./testdata/emit/main.ts"));
      let source_map = module.source_map().await.unwrap();
      assert_eq!(&*source_map, include_bytes!("./testdata/emit/main.ts.map"));
      assert_eq!(module.kind, ModuleKind::JS);
      let module = eszip.get_module("file:///a.ts").unwrap();
      assert_eq!(module.specifier, "file:///b.ts");
      let source = module.source().await;
      assert_eq!(&*source, include_bytes!("./testdata/emit/b.ts"));
      let source_map = module.source_map().await.unwrap();
      assert_eq!(&*source_map, include_bytes!("./testdata/emit/b.ts.map"));
      assert_eq!(module.kind, ModuleKind::JS);

      Ok(())
    };

    tokio::try_join!(fut, test).unwrap();
  }

  #[tokio::test]
  async fn file_format_roundtrippable() {
    let file = tokio::fs::File::open("./src/testdata/redirect.eszip2")
      .await
      .unwrap();
    let (eszip, fut) =
      super::EsZipV2::parse(BufReader::new(file)).await.unwrap();
    fut.await.unwrap();
    let cursor = Cursor::new(eszip.into_bytes());
    let (eszip, fut) =
      super::EsZipV2::parse(BufReader::new(cursor)).await.unwrap();
    fut.await.unwrap();
    let module = eszip.get_module("file:///main.ts").unwrap();
    assert_eq!(module.specifier, "file:///main.ts");
    let source = module.source().await;
    assert_eq!(&*source, include_bytes!("./testdata/emit/main.ts"));
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, include_bytes!("./testdata/emit/main.ts.map"));
    assert_eq!(module.kind, ModuleKind::JS);
    let module = eszip.get_module("file:///a.ts").unwrap();
    assert_eq!(module.specifier, "file:///b.ts");
    let source = module.source().await;
    assert_eq!(&*source, include_bytes!("./testdata/emit/b.ts"));
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, include_bytes!("./testdata/emit/b.ts.map"));
    assert_eq!(module.kind, ModuleKind::JS);
  }
}
