use deno_graph::source::load_data_url;
use deno_graph::source::CacheInfo;
use deno_graph::source::LoadFuture;
use deno_graph::source::Loader;
use deno_graph::source::Resolver;
use deno_graph::GraphOptions;
use deno_graph::ModuleSpecifier;
use eszip::v2::Url;
use futures::io::AsyncRead;
use futures::io::BufReader;
use import_map::ImportMap;
use js_sys::Promise;
use js_sys::Uint8Array;
use std::cell::RefCell;
use std::future::Future;
use std::io::Error;
use std::io::ErrorKind;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::ReadableStreamByobReader;

/// A `Stream` holds a Byob reader and the
/// future of the current `reader.read` operation.
struct Stream {
  inner: Option<ReadableStreamByobReader>,
  fut: Option<JsFuture>,
}

impl Stream {
  fn new(inner: ReadableStreamByobReader) -> Self {
    Self {
      inner: Some(inner),
      fut: None,
    }
  }
}

#[wasm_bindgen]
extern "C" {
  /// Result of a read on BYOB reader.
  /// { value: Uint8Array, done: boolean }
  pub type ReadResult;
  #[wasm_bindgen(method, getter, js_name = done)]
  pub fn is_done(this: &ReadResult) -> bool;

  #[wasm_bindgen(method, getter, js_name = value)]
  pub fn value(this: &ReadResult) -> Option<Uint8Array>;
}

/// A `ParserStream` is a wrapper around
/// Byob stream that also supports reading
/// through in-memory buffers.
///
/// We need this because `#[wasm_bindgen]`
/// structs cannot have type parameters.
enum ParserStream {
  Byob(Stream),
  Buffer(Vec<u8>),
}

impl AsyncRead for ParserStream {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut [u8],
  ) -> Poll<Result<usize, Error>> {
    match *self {
      ParserStream::Byob(ref mut stream) => {
        // If we have a pending future, poll it.
        // otherwise, schedule a new one.
        let fut = match stream.fut.as_mut() {
          Some(fut) => fut,
          None => {
            let length = buf.len();
            let buffer = Uint8Array::new_with_length(length as u32);
            match &stream.inner {
              Some(reader) => {
                let fut =
                  JsFuture::from(reader.read_with_array_buffer_view(&buffer));
                stream.fut.insert(fut)
              }
              None => return Poll::Ready(Ok(0)),
            }
          }
        };
        let result = match Pin::new(fut).poll(cx) {
          Poll::Ready(result) => result,
          Poll::Pending => return Poll::Pending,
        };
        // Clear slot for next `read()`.
        stream.fut = None;

        match result {
          Ok(result) => {
            let result = result.unchecked_into::<ReadResult>();
            match result.is_done() {
              true => {
                // Drop the readable stream.
                stream.inner = None;
                Poll::Ready(Ok(0))
              }
              false => {
                let value = result.value().unwrap_throw();
                let length = value.byte_length() as usize;

                value.copy_to(buf);

                Poll::Ready(Ok(length))
              }
            }
          }
          Err(e) => Poll::Ready(Err(Error::new(
            ErrorKind::Other,
            js_sys::Object::try_from(&e)
              .map(|e| e.to_string().as_string().unwrap_throw())
              .unwrap_or("Unknown error".to_string()),
          ))),
        }
      }
      ParserStream::Buffer(ref mut buffer) => {
        // Put the requested bytes into the buffer and
        // assign the remaining bytes back into the sink.
        let amt = std::cmp::min(buffer.len(), buf.len());
        let (a, b) = buffer.split_at(amt);
        buf[..amt].copy_from_slice(a);
        *buffer = b.to_vec();
        Poll::Ready(Ok(amt))
      }
    }
  }
}

type LoaderFut<T> =
  Pin<Box<dyn Future<Output = Result<BufReader<T>, eszip::ParseError>>>>;
type ParseResult<T> = (eszip::EszipV2, LoaderFut<T>);

#[wasm_bindgen]
pub struct Parser {
  parser: Rc<RefCell<Option<ParseResult<ParserStream>>>>,
}

#[wasm_bindgen]
impl Parser {
  #[wasm_bindgen(constructor)]
  pub fn new() -> Self {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    Self {
      parser: Rc::new(RefCell::new(None)),
    }
  }

  /// Parse from a BYOB readable stream.
  pub fn parse(&self, stream: ReadableStreamByobReader) -> Promise {
    let reader = BufReader::new(ParserStream::Byob(Stream::new(stream)));
    self.parse_reader(reader)
  }

  /// Parse from an in-memory buffer.
  #[wasm_bindgen(js_name = parseBytes)]
  pub fn parse_bytes(&self, buffer: Vec<u8>) -> Promise {
    let reader = BufReader::new(ParserStream::Buffer(buffer));
    self.parse_reader(reader)
  }

  fn parse_reader(&self, reader: BufReader<ParserStream>) -> Promise {
    let parser = Rc::clone(&self.parser);

    wasm_bindgen_futures::future_to_promise(async move {
      let (eszip, loader) = eszip::EszipV2::parse(reader).await.unwrap();
      let specifiers = eszip.specifiers();
      parser.borrow_mut().replace((eszip, Box::pin(loader)));
      Ok(
        specifiers
          .iter()
          .map(JsValue::from)
          .collect::<js_sys::Array>()
          .into(),
      )
    })
  }

  /// Load module sources.
  pub fn load(&mut self) -> Promise {
    let parser = Rc::clone(&self.parser);

    wasm_bindgen_futures::future_to_promise(async move {
      let mut p = parser.borrow_mut();
      let (_, loader) = p.as_mut().unwrap_throw();
      loader.await.unwrap();
      Ok(JsValue::UNDEFINED)
    })
  }

  /// Get a module source.
  #[wasm_bindgen(js_name = getModuleSource)]
  pub fn get_module_source(&self, specifier: String) -> Promise {
    let parser = Rc::clone(&self.parser);

    wasm_bindgen_futures::future_to_promise(async move {
      let p = parser.borrow();
      let (eszip, _) = p.as_ref().unwrap();
      let module = eszip.get_module(&specifier).unwrap();

      // Drop the borrow for the loader
      // to mutably borrow.
      drop(p);
      let source = module.source().await;
      let source = std::str::from_utf8(&source).unwrap();
      Ok(source.to_string().into())
    })
  }

  /// Get a module sourcemap.
  #[wasm_bindgen(js_name = getModuleSourceMap)]
  pub fn get_module_source_map(&self, specifier: String) -> Promise {
    let parser = Rc::clone(&self.parser);

    wasm_bindgen_futures::future_to_promise(async move {
      let p = parser.borrow();
      let (eszip, _) = p.as_ref().unwrap();
      let module = eszip.get_module(&specifier).unwrap();

      // Drop the borrow for the loader
      // to mutably borrow.
      drop(p);
      match module.source_map().await {
        Some(source_map) => {
          let source_map = std::str::from_utf8(&source_map).unwrap();
          Ok(source_map.to_string().into())
        }
        None => Ok(JsValue::NULL),
      }
    })
  }
}

/// Serialize a module graph into eszip.
#[wasm_bindgen(js_name = build)]
pub async fn build_eszip(
  roots: JsValue,
  loader: js_sys::Function,
  import_map_url: JsValue,
) -> Result<Uint8Array, JsValue> {
  std::panic::set_hook(Box::new(console_error_panic_hook::hook));
  let roots: Vec<deno_graph::ModuleSpecifier> =
    serde_wasm_bindgen::from_value(roots)
      .map_err(|e| js_sys::Error::new(&e.to_string()))?;
  let mut loader = GraphLoader(loader);
  let import_map_url: Option<Url> =
    serde_wasm_bindgen::from_value(import_map_url)
      .map_err(|e| js_sys::Error::new(&e.to_string()))?;
  let (maybe_import_map, maybe_import_map_data) =
    if let Some(import_map_url) = import_map_url {
      let resp =
        deno_graph::source::Loader::load(&mut loader, &import_map_url, false)
          .await
          .map_err(|e| js_sys::Error::new(&e.to_string()))?
          .ok_or_else(|| {
            js_sys::Error::new(&format!(
              "import map not found at '{import_map_url}'"
            ))
          })?;
      match resp {
        deno_graph::source::LoadResponse::Module {
          specifier, content, ..
        } => {
          let import_map =
            import_map::parse_from_json(&specifier, &content).unwrap();
          (Some(import_map.import_map), Some((specifier, content)))
        }
        _ => unimplemented!(),
      }
    } else {
      (None, None)
    };
  let resolver = GraphResolver(maybe_import_map);
  let analyzer = deno_graph::CapturingModuleAnalyzer::default();
  let graph = deno_graph::create_graph(
    roots,
    &mut loader,
    GraphOptions {
      resolver: Some(&resolver),
      module_analyzer: Some(&analyzer),
      ..Default::default()
    },
  )
  .await;
  graph
    .valid()
    .map_err(|e| js_sys::Error::new(&e.to_string()))?;
  let mut eszip = eszip::EszipV2::from_graph(
    graph,
    &analyzer.as_capturing_parser(),
    Default::default(),
  )
  .map_err(|e| js_sys::Error::new(&e.to_string()))?;
  if let Some((import_map_specifier, import_map_content)) =
    maybe_import_map_data
  {
    eszip.add_import_map(
      import_map_specifier.to_string(),
      Arc::new(import_map_content.as_bytes().to_vec()),
    )
  }
  Ok(Uint8Array::from(eszip.into_bytes().as_slice()))
}

// Taken from deno_graph
// https://github.com/denoland/deno_graph/blob/main/src/js_graph.rs#L43
pub struct GraphLoader(js_sys::Function);

impl Loader for GraphLoader {
  fn get_cache_info(&self, _: &ModuleSpecifier) -> Option<CacheInfo> {
    None
  }

  fn load(
    &mut self,
    specifier: &ModuleSpecifier,
    is_dynamic: bool,
  ) -> LoadFuture {
    if specifier.scheme() == "data" {
      Box::pin(std::future::ready(load_data_url(specifier)))
    } else {
      let specifier = specifier.clone();
      let result = self.0.call2(
        &JsValue::null(),
        &JsValue::from(specifier.to_string()),
        &JsValue::from(is_dynamic),
      );
      Box::pin(async move {
        let response = match result {
          Ok(result) => {
            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(
              &result,
            ))
            .await
          }
          Err(err) => Err(err),
        };
        response
          .map(|value| serde_wasm_bindgen::from_value(value).unwrap())
          .map_err(|err| {
            anyhow::anyhow!(err
              .as_string()
              .unwrap_or_else(|| "an error occured during loading".to_string()))
          })
      })
    }
  }
}

#[derive(Debug)]
pub struct GraphResolver(Option<ImportMap>);

impl Resolver for GraphResolver {
  fn resolve(
    &self,
    specifier: &str,
    referrer: &deno_graph::ModuleSpecifier,
  ) -> Result<deno_graph::ModuleSpecifier, anyhow::Error> {
    if let Some(import_map) = &self.0 {
      Ok(import_map.resolve(specifier, referrer)?)
    } else {
      Ok(deno_graph::resolve_import(specifier, referrer)?)
    }
  }
}
