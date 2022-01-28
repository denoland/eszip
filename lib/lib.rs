use js_sys::Promise;
use js_sys::Uint8Array;
use std::cell::RefCell;
use std::future::Future;
use std::io::Error;
use std::io::ErrorKind;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Context;
use std::task::Poll;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::io::ReadBuf;
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
    buf: &mut ReadBuf<'_>,
  ) -> Poll<Result<(), Error>> {
    match *self {
      ParserStream::Byob(ref mut stream) => {
        // If we have a pending future, poll it.
        // otherwise, schedule a new one.
        let fut = match stream.fut.as_mut() {
          Some(fut) => fut,
          None => {
            let length = buf.remaining();
            let buffer = Uint8Array::new_with_length(length as u32);
            match &stream.inner {
              Some(reader) => {
                let fut =
                  JsFuture::from(reader.read_with_array_buffer_view(&buffer));
                stream.fut.insert(fut)
              }
              None => return Poll::Ready(Ok(())),
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
                Poll::Ready(Ok(()))
              }
              false => {
                let value = result.value().unwrap_throw();
                let length = value.byte_length() as usize;

                let mut bytes = vec![0; length];
                value.copy_to(&mut bytes);
                buf.put_slice(&bytes);

                Poll::Ready(Ok(()))
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
        let amt = std::cmp::min(buffer.len(), buf.remaining());
        let (a, b) = buffer.split_at(amt);
        buf.put_slice(a);
        *buffer = b.to_vec();
        Poll::Ready(Ok(()))
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
  #[wasm_bindgen(js_name = parseBuffer)]
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

#[wasm_bindgen]
extern "C" {
  pub type Module;
  #[wasm_bindgen(method, getter)]
  pub fn specifier(this: &Module) -> String;
  #[wasm_bindgen(method, getter)]
  pub fn kind(this: &Module) -> String;
  #[wasm_bindgen(method, getter)]
  pub fn maybe_source(this: &Module) -> Option<String>;
  #[wasm_bindgen(method, getter)]
  pub fn maybe_parsed_source(this: &Module) -> Option<String>;
  #[wasm_bindgen(method, getter)]
  pub fn media_type(this: &Module) -> String;
  #[wasm_bindgen(method, getter)]
  pub fn dependencies(this: &Module) -> js_sys::Array;
}

#[wasm_bindgen]
extern "C" {
  pub type ModuleGraph;
  #[wasm_bindgen(method)]
  pub fn get(this: &ModuleGraph, specifier: String) -> Module;
  #[wasm_bindgen(method)]
  pub fn roots(this: &ModuleGraph) -> js_sys::Array;
  #[wasm_bindgen(method)]
  pub fn redirects(this: &ModuleGraph) -> js_sys::Array;
}

use eszip::deno_ast::TranspiledSource;
use eszip::v2::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

// BIG TODO for tommorow :)
#[wasm_bindgen]
pub fn from_deno_graph(graph: ModuleGraph) -> Result<Vec<u8>, JsValue> {
  let mut modules = HashMap::new();
  let mut ordered_modules = vec![];

  fn visit_module(
    graph: &ModuleGraph,
    modules: &mut HashMap<String, EszipV2Module>,
    ordered_modules: &mut Vec<String>,
    specifier: &Url,
  ) -> Result<(), anyhow::Error> {
    let module = graph.get(specifier.to_string());
    let specifier = module.specifier().as_str();
    if modules.contains_key(specifier) {
      return Ok(());
    }

    match module.kind() {
      deno_graph::ModuleKind::Esm => {
        let (source, source_map) = match module.media_type() {
          deno_graph::MediaType::JavaScript | deno_graph::MediaType::Mjs => {
            let source = module.maybe_source().as_ref().unwrap();
            (source.as_bytes().to_owned(), vec![])
          }
          deno_graph::MediaType::Jsx
          | deno_graph::MediaType::TypeScript
          | deno_graph::MediaType::Mts
          | deno_graph::MediaType::Tsx
          | deno_graph::MediaType::Dts
          | deno_graph::MediaType::Dmts => {
            let parsed_source = module.maybe_parsed_source().as_ref().unwrap();
            let TranspiledSource {
              text: source,
              source_map: maybe_source_map,
            } = parsed_source.transpile(Default::default())?;
            let source_map = maybe_source_map.unwrap_or_default();
            (source.into_bytes(), source_map.into_bytes())
          }
          _ => {
            return Err(anyhow::anyhow!(
              "unsupported media type {} for {}",
              module.media_type(),
              specifier
            ));
          }
        };

        let specifier = module.specifier();
        let module = EszipV2Module::Module {
          kind: ModuleKind::JavaScript,
          source: EszipV2SourceSlot::Ready(Arc::new(source)),
          source_map: EszipV2SourceSlot::Ready(Arc::new(source_map)),
        };
        modules.insert(specifier, module);
      }
      deno_graph::ModuleKind::Asserted => {
        if module.media_type() == deno_graph::MediaType::Json {
          let source = module.maybe_source().as_ref().unwrap();
          let specifier = module.specifier();
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
      _ => {}
    }

    ordered_modules.push(specifier.to_string());
    for dep in module.dependencies() {
      if let Some(specifier) = dep.get_code() {
        visit_module(graph, modules, ordered_modules, specifier).unwrap();
      }
    }

    Ok(())
  }

  for root in &graph.roots() {
    visit_module(&graph, &mut modules, &mut ordered_modules, root).unwrap();
  }

  for (specifier, target) in graph.redirects() {
    let module = EszipV2Module::Redirect {
      target: target.to_string(),
    };
    modules.insert(specifier.to_string(), module);
  }

  let eszip = EszipV2 {
    modules: Arc::new(Mutex::new(modules)),
    ordered_modules,
  };
  Ok(eszip.into_bytes())
}
