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
  pub type ReadResult;
  #[wasm_bindgen(method, getter, js_name = done)]
  pub fn is_done(this: &ReadResult) -> bool;
  #[wasm_bindgen(method, getter, js_name = value)]
  pub fn value(this: &ReadResult) -> Option<Uint8Array>;
}

impl AsyncRead for Stream {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<Result<(), Error>> {
    let fut = match self.fut.as_mut() {
      Some(fut) => fut,
      None => {
        let length = buf.remaining();

        let buffer = Uint8Array::new_with_length(length as u32);
        match &self.inner {
          Some(reader) => {
            let fut =
              JsFuture::from(reader.read_with_array_buffer_view(&buffer));
            self.fut.insert(fut)
          }
          None => return Poll::Ready(Ok(())),
        }
      }
    };
    let result = match Pin::new(fut).poll(cx) {
      Poll::Ready(result) => result,
      Poll::Pending => return Poll::Pending,
    };
    self.fut = None;

    match result {
      Ok(result) => {
        let result = result.unchecked_into::<ReadResult>();
        match result.is_done() {
          true => {
            self.inner = None;
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
}

#[wasm_bindgen]
pub struct Parser {
  parser: Rc<
    RefCell<
      Option<(
        eszip::EszipV2,
        Pin<
          Box<
            dyn Future<Output = Result<BufReader<Stream>, eszip::ParseError>>,
          >,
        >,
      )>,
    >,
  >,
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

  pub fn parse(&self, stream: ReadableStreamByobReader) -> Promise {
    let reader = BufReader::new(Stream::new(stream));
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

  pub fn get_module_source(&mut self, specifier: String) -> Promise {
    let parser = Rc::clone(&self.parser);
    wasm_bindgen_futures::future_to_promise(async move {
      let mut p = parser.borrow_mut();
      let (eszip, loader) = p.as_mut().unwrap();
      let module = eszip.get_module(&specifier).unwrap();

      loader.await;
      let source = module.source().await;
      let source = std::str::from_utf8(&source).unwrap();
      Ok(source.to_string().into())
    })
  }
}
