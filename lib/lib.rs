use futures::io::Error;
use futures::task::Context;
use futures::task::Poll;
use futures::AsyncRead;
use futures::FutureExt;
use js_sys::Uint8Array;
use std::pin::Pin;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

#[cfg(web_sys_unstable_apis)]
use web_sys::ReadableStreamByobReader;

struct Stream {
  inner: ReadableStreamByobReader,
  fut: Option<JsFuture>,
}

impl Stream {
  fn new(inner: ReadableStreamByobReader) -> Self {
    Self { inner, fut: None }
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
    buf: &mut [u8],
  ) -> Poll<Result<usize, std::io::Error>> {
    let fut = match self.fut.as_mut() {
      Some(fut) => fut,
      None => {
        let buffer = Uint8Array::new_with_length(buf.len() as u32);
        let fut =
          JsFuture::from(self.inner.read_with_array_buffer_view(&buffer));
        self.fut.insert(fut)
      }
    };

    let result = futures::ready!(fut.poll_unpin(cx));
    self.fut = None;

    match result {
      Ok(result) => {
        let result = result.unchecked_into::<ReadResult>();
        match result.is_done() {
          true => Poll::Ready(Ok(0)),
          false => {
            let value = result.value().unwrap_throw();
            let length = value.byte_length() as usize;
            value.copy_to(&mut buf[0..length]);
            Poll::Ready(Ok(length))
          }
        }
      }
      // TODO(@littledivy): handle error
      Err(_) => todo!(),
    }
  }
}

#[wasm_bindgen]
pub async fn eszip_parse(reader: ReadableStreamByobReader) {
  let stream = Stream::new(reader);
  let eszip = eszip::EszipV2::parse(stream);
}
