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
use deno_graph::CapturingModuleParser;
use deno_graph::ModuleGraph;
use deno_graph::ModuleParser;
use futures::future::poll_fn;
use sha2::Digest;
use sha2::Sha256;
use tokio::io::AsyncReadExt;
pub use url::Url;

use crate::error::FromGraphError;
use crate::error::ParseError;
use crate::Module;
use crate::ModuleInner;
pub use crate::ModuleKind;

pub(crate) const ESZIP_V2_MAGIC: &[u8; 8] = b"ESZIP_V2";

#[derive(Debug, PartialEq)]
#[repr(u8)]
enum HeaderFrameKind {
  Module = 0,
  Redirect = 1,
}

/// Version 2 of the Eszip format. This format supports streaming sources and
/// source maps.
#[derive(Debug, Default)]
pub struct EszipV2 {
  modules: Arc<Mutex<HashMap<String, EszipV2Module>>>,
  pub ordered_modules: Vec<String>,
}

#[derive(Debug)]
pub enum EszipV2Module {
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
pub enum EszipV2SourceSlot {
  Pending {
    offset: usize,
    length: usize,
    wakers: Vec<Waker>,
  },
  Ready(Arc<Vec<u8>>),
}

impl EszipV2SourceSlot {
  fn bytes(&self) -> &[u8] {
    match self {
      EszipV2SourceSlot::Ready(v) => v,
      _ => panic!("EszipV2SourceSlot::bytes() called on a pending slot"),
    }
  }
}

impl EszipV2 {
  /// Parse a EszipV2 from an AsyncRead stream. This function returns once the
  /// header section of the eszip has been parsed. Once this function returns,
  /// the data section will not necessarially have been parsed yet. To parse
  /// the data section, poll/await the future returned in the second tuple slot.
  pub async fn parse<R: tokio::io::AsyncRead + Unpin>(
    mut reader: tokio::io::BufReader<R>,
  ) -> Result<
    (
      EszipV2,
      impl Future<Output = Result<tokio::io::BufReader<R>, ParseError>>,
    ),
    ParseError,
  > {
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

    // This macro reads n number of bytes from the header section. If the header
    // section is not long enough, this function will be early exited with an
    // error.
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
            0 => ModuleKind::JavaScript,
            1 => ModuleKind::Json,
            n => return Err(ParseError::InvalidV2ModuleKind(n, read)),
          };
          let source = if source_offset == 0 && source_len == 0 {
            EszipV2SourceSlot::Ready(Arc::new(vec![]))
          } else {
            EszipV2SourceSlot::Pending {
              offset: source_offset as usize,
              length: source_len as usize,
              wakers: vec![],
            }
          };
          let source_map = if source_map_offset == 0 && source_map_len == 0 {
            EszipV2SourceSlot::Ready(Arc::new(vec![]))
          } else {
            EszipV2SourceSlot::Pending {
              offset: source_map_offset as usize,
              length: source_map_len as usize,
              wakers: vec![],
            }
          };
          let module = EszipV2Module::Module {
            kind,
            source,
            source_map,
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
            let slot = std::mem::replace(
              source,
              EszipV2SourceSlot::Ready(Arc::new(source_bytes)),
            );

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
              EszipV2SourceSlot::Ready(Arc::new(source_map_bytes)),
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

      Ok(reader)
    };

    Ok((
      EszipV2 {
        modules,
        ordered_modules: vec![], // TODO
      },
      fut,
    ))
  }

  /// Add an import map to the eszip archive. The import map will always be
  /// placed at the top of the archive, so it can be read before any other
  /// modules are loaded.
  ///
  /// If a module with this specifier is already present, then this is a no-op
  /// (except that this specifier will now be at the top of the archive).
  pub fn add_import_map(&mut self, specifier: String, source: Arc<Vec<u8>>) {
    let module = EszipV2Module::Module {
      kind: ModuleKind::Json,
      source: EszipV2SourceSlot::Ready(source),
      source_map: EszipV2SourceSlot::Ready(Arc::new(vec![])),
    };
    let mut modules = self.modules.lock().unwrap();
    if modules.get(&specifier).is_none() {
      modules.insert(specifier.clone(), module);
    }
    let mut module_ordering =
      Vec::with_capacity(self.ordered_modules.len() + 1);
    module_ordering.push(specifier);
    let old_module_ordering =
      std::mem::replace(&mut self.ordered_modules, module_ordering);
    self.ordered_modules.extend_from_slice(&old_module_ordering);
  }

  /// Serialize the eszip archive into a byte buffer.
  pub fn into_bytes(self) -> Vec<u8> {
    let mut header: Vec<u8> = ESZIP_V2_MAGIC.to_vec();
    header.extend_from_slice(&[0u8; 4]); // add 4 bytes of space to put the header length in later
    let mut sources: Vec<u8> = Vec::new();
    let mut source_maps: Vec<u8> = Vec::new();

    let modules = self.modules.lock().unwrap();

    let mut ordered_modules = self.ordered_modules.clone();
    let seen_modules: HashSet<String> =
      self.ordered_modules.into_iter().collect();

    for specifier in modules.keys() {
      let specifier = specifier.as_str();
      if seen_modules.contains(specifier) {
        continue;
      }
      ordered_modules.push(specifier.to_string());
    }

    for specifier in ordered_modules {
      let module = modules.get(&specifier).unwrap();

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
          if source_length > 0 {
            let source_offset = sources.len() as u32;
            sources.extend_from_slice(source_bytes);
            let mut hasher = Sha256::new();
            hasher.update(source_bytes);
            let source_hash = hasher.finalize();
            sources.extend_from_slice(&source_hash);

            header.extend_from_slice(&source_offset.to_be_bytes());
            header.extend_from_slice(&source_length.to_be_bytes());
          } else {
            header.extend_from_slice(&0u32.to_be_bytes());
            header.extend_from_slice(&0u32.to_be_bytes());
          }

          // add the source map to the `source_maps` bytes
          let source_map_bytes = source_map.bytes();
          let source_map_length = source_map_bytes.len() as u32;
          if source_map_length > 0 {
            let source_map_offset = source_maps.len() as u32;
            source_maps.extend_from_slice(source_map_bytes);
            let mut hasher = Sha256::new();
            hasher.update(source_map_bytes);
            let source_map_hash = hasher.finalize();
            source_maps.extend_from_slice(&source_map_hash);

            header.extend_from_slice(&source_map_offset.to_be_bytes());
            header.extend_from_slice(&source_map_length.to_be_bytes());
          } else {
            header.extend_from_slice(&0u32.to_be_bytes());
            header.extend_from_slice(&0u32.to_be_bytes());
          }

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

  /// Turn a [deno_graph::ModuleGraph] into an [EszipV2]. All modules from the
  /// graph will be transpiled and stored in the eszip archive.
  ///
  /// The ordering of the modules in the graph is dependant on the module graph
  /// tree. The root module is added to the top of the archive, and the leaves
  /// to the end. This allows for efficient deserialization of the archive right
  /// into an isolate.
  pub fn from_graph(
    graph: ModuleGraph,
    parser: &CapturingModuleParser,
    mut emit_options: EmitOptions,
  ) -> Result<Self, FromGraphError> {
    emit_options.inline_sources = true;
    emit_options.inline_source_map = false;
    emit_options.source_map = true;

    let mut modules = HashMap::new();

    let mut ordered_modules = vec![];

    fn visit_module(
      graph: &ModuleGraph,
      parser: &CapturingModuleParser,
      emit_options: &EmitOptions,
      modules: &mut HashMap<String, EszipV2Module>,
      ordered_modules: &mut Vec<String>,
      specifier: &Url,
    ) -> Result<(), FromGraphError> {
      let module = graph.get(specifier).unwrap();
      let specifier = module.specifier.as_str();
      if modules.contains_key(specifier) {
        return Ok(());
      }

      match module.kind {
        deno_graph::ModuleKind::Esm => {
          let (source, source_map) = match module.media_type {
            deno_graph::MediaType::JavaScript | deno_graph::MediaType::Mjs => {
              let source = module.maybe_source.as_ref().unwrap();
              (source.as_bytes().to_owned(), vec![])
            }
            deno_graph::MediaType::Jsx
            | deno_graph::MediaType::TypeScript
            | deno_graph::MediaType::Mts
            | deno_graph::MediaType::Tsx
            | deno_graph::MediaType::Dts
            | deno_graph::MediaType::Dmts => {
              let parsed_source = parser
                .parse_module(
                  &module.specifier,
                  module.maybe_source.clone().unwrap(),
                  module.media_type,
                )
                .map_err(|diagnostic| {
                  FromGraphError::Parse(module.specifier.clone(), diagnostic)
                })?;
              let TranspiledSource {
                text: source,
                source_map: maybe_source_map,
              } = parsed_source.transpile(emit_options).map_err(|err| {
                FromGraphError::Emit(module.specifier.clone(), err)
              })?;
              let source_map = maybe_source_map.unwrap_or_default();
              (source.into_bytes(), source_map.into_bytes())
            }
            _ => {
              return Err(FromGraphError::UnsupportedMediaType(
                module.specifier.clone(),
                module.media_type,
              ));
            }
          };

          let specifier = module.specifier.to_string();
          let module = EszipV2Module::Module {
            kind: ModuleKind::JavaScript,
            source: EszipV2SourceSlot::Ready(Arc::new(source)),
            source_map: EszipV2SourceSlot::Ready(Arc::new(source_map)),
          };
          modules.insert(specifier, module);
        }
        deno_graph::ModuleKind::Asserted => {
          if module.media_type == deno_graph::MediaType::Json {
            let source = module.maybe_source.as_ref().unwrap();
            let specifier = module.specifier.to_string();
            let module = EszipV2Module::Module {
              kind: ModuleKind::Json,
              source: EszipV2SourceSlot::Ready(Arc::new(
                source.as_bytes().to_owned(),
              )),
              source_map: EszipV2SourceSlot::Ready(Arc::new(vec![])),
            };
            modules.insert(specifier, module);
          }
        }
        deno_graph::ModuleKind::External | deno_graph::ModuleKind::BuiltIn => {
          return Ok(())
        }
        _ => {}
      }

      ordered_modules.push(specifier.to_string());
      for dep in module.dependencies.values() {
        if dep.is_dynamic {
          continue;
        }
        if let Some(specifier) = dep.get_code() {
          visit_module(
            graph,
            parser,
            emit_options,
            modules,
            ordered_modules,
            specifier,
          )?;
        }
      }

      Ok(())
    }

    for (root, kind) in &graph.roots {
      assert!(matches!(
        kind,
        deno_graph::ModuleKind::Esm | deno_graph::ModuleKind::Asserted
      ));
      visit_module(
        &graph,
        parser,
        &emit_options,
        &mut modules,
        &mut ordered_modules,
        root,
      )?;
    }

    for (specifier, target) in &graph.redirects {
      let module = EszipV2Module::Redirect {
        target: target.to_string(),
      };
      modules.insert(specifier.to_string(), module);
    }

    Ok(Self {
      modules: Arc::new(Mutex::new(modules)),
      ordered_modules,
    })
  }

  /// Get the module metadata for a given module specifier. This function will
  /// follow redirects. The returned module has functions that can be used to
  /// obtain the module source and source map.
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
            inner: ModuleInner::V2(EszipV2 {
              modules: self.modules.clone(),
              ordered_modules: vec![],
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
  ) -> Arc<Vec<u8>> {
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
        EszipV2SourceSlot::Ready(bytes) => Poll::Ready(bytes.clone()),
      }
    })
    .await
  }

  pub(crate) async fn get_module_source_map<'a>(
    &'a self,
    specifier: &str,
  ) -> Arc<Vec<u8>> {
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
        EszipV2SourceSlot::Ready(bytes) => Poll::Ready(bytes.clone()),
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

  use deno_ast::EmitOptions;
  use deno_graph::source::LoadResponse;
  use deno_graph::source::ResolveResponse;
  use deno_graph::CapturingModuleAnalyzer;
  use deno_graph::ModuleSpecifier;
  use import_map::ImportMap;
  use pretty_assertions::assert_eq;
  use tokio::io::BufReader;
  use url::Url;

  use crate::ModuleKind;

  struct FileLoader;

  macro_rules! assert_matches_file {
    ($source:ident, $file:literal) => {
      assert_eq!(
        String::from_utf8($source.to_vec()).unwrap(),
        include_str!($file)
      );
    };
  }

  impl deno_graph::source::Loader for FileLoader {
    fn load(
      &mut self,
      specifier: &ModuleSpecifier,
      is_dynamic: bool,
    ) -> deno_graph::source::LoadFuture {
      if is_dynamic {
        return Box::pin(async { Ok(None) });
      }
      assert_eq!(specifier.scheme(), "file");
      let path = format!("./src/testdata/source{}", specifier.path());
      Box::pin(async move {
        let path = Path::new(&path);
        let resolved = path.canonicalize().unwrap();
        let source = tokio::fs::read_to_string(&resolved).await.unwrap();
        let specifier =
          resolved.file_name().unwrap().to_string_lossy().to_string();
        let specifier = Url::parse(&format!("file:///{}", specifier)).unwrap();
        Ok(Some(LoadResponse::Module {
          content: source.into(),
          maybe_headers: None,
          specifier,
        }))
      })
    }
  }

  #[derive(Debug)]
  struct ImportMapResolver(ImportMap);

  impl deno_graph::source::Resolver for ImportMapResolver {
    fn resolve(
      &self,
      specifier: &str,
      referrer: &ModuleSpecifier,
    ) -> ResolveResponse {
      match self.0.resolve(specifier, referrer) {
        Ok(specifier) => ResolveResponse::Specifier(specifier),
        Err(err) => ResolveResponse::Err(err.into()),
      }
    }
  }

  #[tokio::test]
  async fn test_graph_external() {
    let roots = vec![(
      ModuleSpecifier::parse("file:///external.ts").unwrap(),
      deno_graph::ModuleKind::Esm,
    )];

    struct ExternalLoader;

    impl deno_graph::source::Loader for ExternalLoader {
      fn load(
        &mut self,
        specifier: &ModuleSpecifier,
        is_dynamic: bool,
      ) -> deno_graph::source::LoadFuture {
        if is_dynamic {
          unreachable!();
        }
        let scheme = specifier.scheme();
        if scheme == "extern" {
          let specifier = specifier.clone();
          return Box::pin(async move {
            Ok(Some(LoadResponse::External { specifier }))
          });
        }
        assert_eq!(scheme, "file");
        let path = format!("./src/testdata/source{}", specifier.path());
        Box::pin(async move {
          let path = Path::new(&path);
          let resolved = path.canonicalize().unwrap();
          let source = tokio::fs::read_to_string(&resolved).await.unwrap();
          let specifier =
            resolved.file_name().unwrap().to_string_lossy().to_string();
          let specifier =
            Url::parse(&format!("file:///{}", specifier)).unwrap();
          Ok(Some(LoadResponse::Module {
            content: source.into(),
            maybe_headers: None,
            specifier,
          }))
        })
      }
    }

    let analyzer = CapturingModuleAnalyzer::default();
    let graph = deno_graph::create_graph(
      roots,
      false,
      None,
      &mut ExternalLoader,
      None,
      None,
      Some(&analyzer),
      None,
    )
    .await;
    graph.valid().unwrap();
    let eszip = super::EszipV2::from_graph(
      graph,
      &analyzer.as_capturing_parser(),
      EmitOptions::default(),
    )
    .unwrap();
    let module = eszip.get_module("file:///external.ts").unwrap();
    assert_eq!(module.specifier, "file:///external.ts");
    assert!(eszip.get_module("external:fs").is_none());
  }

  #[tokio::test]
  async fn from_graph_redirect() {
    let roots = vec![(
      ModuleSpecifier::parse("file:///main.ts").unwrap(),
      deno_graph::ModuleKind::Esm,
    )];
    let analyzer = CapturingModuleAnalyzer::default();
    let graph = deno_graph::create_graph(
      roots,
      false,
      None,
      &mut FileLoader,
      None,
      None,
      Some(&analyzer),
      None,
    )
    .await;
    graph.valid().unwrap();
    let eszip = super::EszipV2::from_graph(
      graph,
      &analyzer.as_capturing_parser(),
      EmitOptions::default(),
    )
    .unwrap();
    let module = eszip.get_module("file:///main.ts").unwrap();
    assert_eq!(module.specifier, "file:///main.ts");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/emit/main.ts");
    let source_map = module.source_map().await.unwrap();
    assert_matches_file!(source_map, "./testdata/emit/main.ts.map");
    assert_eq!(module.kind, ModuleKind::JavaScript);
    let module = eszip.get_module("file:///a.ts").unwrap();
    assert_eq!(module.specifier, "file:///b.ts");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/emit/b.ts");
    let source_map = module.source_map().await.unwrap();
    assert_matches_file!(source_map, "./testdata/emit/b.ts.map");
    assert_eq!(module.kind, ModuleKind::JavaScript);
  }

  #[tokio::test]
  async fn from_graph_json() {
    let roots = vec![(
      ModuleSpecifier::parse("file:///json.ts").unwrap(),
      deno_graph::ModuleKind::Esm,
    )];
    let analyzer = CapturingModuleAnalyzer::default();
    let graph = deno_graph::create_graph(
      roots,
      false,
      None,
      &mut FileLoader,
      None,
      None,
      Some(&analyzer),
      None,
    )
    .await;
    graph.valid().unwrap();
    let eszip = super::EszipV2::from_graph(
      graph,
      &analyzer.as_capturing_parser(),
      EmitOptions::default(),
    )
    .unwrap();
    let module = eszip.get_module("file:///json.ts").unwrap();
    assert_eq!(module.specifier, "file:///json.ts");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/emit/json.ts");
    let _source_map = module.source_map().await.unwrap();
    assert_eq!(module.kind, ModuleKind::JavaScript);
    let module = eszip.get_module("file:///data.json").unwrap();
    assert_eq!(module.specifier, "file:///data.json");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/emit/data.json");
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, &[0; 0]);
    assert_eq!(module.kind, ModuleKind::Json);
  }

  #[tokio::test]
  async fn from_graph_dynamic() {
    let roots = vec![(
      ModuleSpecifier::parse("file:///dynamic.ts").unwrap(),
      deno_graph::ModuleKind::Esm,
    )];
    let analyzer = CapturingModuleAnalyzer::default();
    let graph = deno_graph::create_graph(
      roots,
      false,
      None,
      &mut FileLoader,
      None,
      None,
      Some(&analyzer),
      None,
    )
    .await;
    graph.valid().unwrap();
    let eszip = super::EszipV2::from_graph(
      graph,
      &analyzer.as_capturing_parser(),
      EmitOptions::default(),
    )
    .unwrap();
    let module = eszip.get_module("file:///dynamic.ts").unwrap();
    assert_eq!(module.specifier, "file:///dynamic.ts");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/emit/dynamic.ts");
    let _source_map = module.source_map().await.unwrap();
    assert_eq!(module.kind, ModuleKind::JavaScript);
    let module = eszip.get_module("file:///data.json");
    assert!(module.is_none());
  }

  #[tokio::test]
  async fn file_format_parse_redirect() {
    let file = tokio::fs::File::open("./src/testdata/redirect.eszip2")
      .await
      .unwrap();
    let (eszip, fut) =
      super::EszipV2::parse(BufReader::new(file)).await.unwrap();

    let test = async move {
      let module = eszip.get_module("file:///main.ts").unwrap();
      assert_eq!(module.specifier, "file:///main.ts");
      let source = module.source().await;
      assert_matches_file!(source, "./testdata/redirect_data/main.ts");
      let source_map = module.source_map().await.unwrap();
      assert_matches_file!(source_map, "./testdata/redirect_data/main.ts.map");
      assert_eq!(module.kind, ModuleKind::JavaScript);
      let module = eszip.get_module("file:///a.ts").unwrap();
      assert_eq!(module.specifier, "file:///b.ts");
      let source = module.source().await;
      assert_matches_file!(source, "./testdata/redirect_data/b.ts");
      let source_map = module.source_map().await.unwrap();
      assert_matches_file!(source_map, "./testdata/redirect_data/b.ts.map");
      assert_eq!(module.kind, ModuleKind::JavaScript);

      Ok(())
    };

    tokio::try_join!(fut, test).unwrap();
  }

  #[tokio::test]
  async fn file_format_parse_json() {
    let file = tokio::fs::File::open("./src/testdata/json.eszip2")
      .await
      .unwrap();
    let (eszip, fut) =
      super::EszipV2::parse(BufReader::new(file)).await.unwrap();

    let test = async move {
      let module = eszip.get_module("file:///json.ts").unwrap();
      assert_eq!(module.specifier, "file:///json.ts");
      let source = module.source().await;
      assert_matches_file!(source, "./testdata/emit/json.ts");
      let _source_map = module.source_map().await.unwrap();
      assert_eq!(module.kind, ModuleKind::JavaScript);
      let module = eszip.get_module("file:///data.json").unwrap();
      assert_eq!(module.specifier, "file:///data.json");
      let source = module.source().await;
      assert_matches_file!(source, "./testdata/emit/data.json");
      let source_map = module.source_map().await.unwrap();
      assert_eq!(&*source_map, &[0; 0]);
      assert_eq!(module.kind, ModuleKind::Json);

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
      super::EszipV2::parse(BufReader::new(file)).await.unwrap();
    fut.await.unwrap();
    let cursor = Cursor::new(eszip.into_bytes());
    let (eszip, fut) =
      super::EszipV2::parse(BufReader::new(cursor)).await.unwrap();
    fut.await.unwrap();
    let module = eszip.get_module("file:///main.ts").unwrap();
    assert_eq!(module.specifier, "file:///main.ts");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/redirect_data/main.ts");
    let source_map = module.source_map().await.unwrap();
    assert_matches_file!(source_map, "./testdata/redirect_data/main.ts.map");
    assert_eq!(module.kind, ModuleKind::JavaScript);
    let module = eszip.get_module("file:///a.ts").unwrap();
    assert_eq!(module.specifier, "file:///b.ts");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/redirect_data/b.ts");
    let source_map = module.source_map().await.unwrap();
    assert_matches_file!(source_map, "./testdata/redirect_data/b.ts.map");
    assert_eq!(module.kind, ModuleKind::JavaScript);
  }

  #[tokio::test]
  async fn import_map() {
    let mut loader = FileLoader;
    let resp = deno_graph::source::Loader::load(
      &mut loader,
      &Url::parse("file:///import_map.json").unwrap(),
      false,
    )
    .await
    .unwrap()
    .unwrap();
    let (specifier, content) = match resp {
      deno_graph::source::LoadResponse::Module {
        specifier, content, ..
      } => (specifier, content),
      _ => unimplemented!(),
    };
    let import_map = import_map::parse_from_json(&specifier, &content).unwrap();
    let roots = vec![(
      ModuleSpecifier::parse("file:///mapped.js").unwrap(),
      deno_graph::ModuleKind::Esm,
    )];
    let analyzer = CapturingModuleAnalyzer::default();
    let graph = deno_graph::create_graph(
      roots,
      false,
      None,
      &mut FileLoader,
      Some(&ImportMapResolver(import_map.import_map)),
      None,
      Some(&analyzer),
      None,
    )
    .await;
    graph.valid().unwrap();
    let mut eszip = super::EszipV2::from_graph(
      graph,
      &analyzer.as_capturing_parser(),
      EmitOptions::default(),
    )
    .unwrap();
    let import_map_bytes = Arc::new(content.as_bytes().to_vec());
    eszip.add_import_map(specifier.to_string(), import_map_bytes);

    let module = eszip.get_module("file:///import_map.json").unwrap();
    assert_eq!(module.specifier, "file:///import_map.json");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/source/import_map.json");
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, &[0; 0]);
    assert_eq!(module.kind, ModuleKind::Json);

    let module = eszip.get_module("file:///mapped.js").unwrap();
    assert_eq!(module.specifier, "file:///mapped.js");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/source/mapped.js");
    let source_map = module.source_map().await.unwrap();
    assert_eq!(&*source_map, &[0; 0]);
    assert_eq!(module.kind, ModuleKind::JavaScript);

    let module = eszip.get_module("file:///a.ts").unwrap();
    assert_eq!(module.specifier, "file:///b.ts");
    let source = module.source().await;
    assert_matches_file!(source, "./testdata/emit/b.ts");
    let source_map = module.source_map().await.unwrap();
    assert_matches_file!(source_map, "./testdata/emit/b.ts.map");
    assert_eq!(module.kind, ModuleKind::JavaScript);
  }
}
