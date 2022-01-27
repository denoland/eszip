use futures::io::AsyncRead;
use futures::io::Error;
use futures::task::Context;
use futures::task::Poll;
use js_sys::ArrayBuffer;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use web_sys::ReadableStreamBYOBReader;
use std::pin::Pin;

struct Stream {
  inner: ReadableStreamBYOBReader,
}

impl Stream {
  fn new(inner: ReadableStreamBYOBReader) -> Self {
    Self {
      inner,
      buffer: Uint8Array::new(),
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
    buf: &mut [u8],
  ) -> Poll<Result<usize, std::io::Error>> {
    let fut = match self.fut.as_mut() {
      Some(fut) => fut,
      None => {
        let buffer = Uint8Array::new_with_length(buf.len())
          .unchecked_into::<ArrayBufferView>();
        let fut = JsFuture::from(self.inner.read_with_array_buffer_view(&buffer));
        self.fut.insert(fut)
      }
    };

    let result = futures::ready!(fut.poll_unpin(cx));
    self.fut = None;

    match js_ready {
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
      },
      // TODO(@littledivy): handle error
      Err(_) => todo!(),
    }
  }
}

#[wasm_bindgen]
pub async fn eszip_parse(stream: ReadableStreamBYOBReader) {}
