use deno_core::anyhow::Error;
use deno_core::futures::FutureExt;
use deno_core::futures::StreamExt;
use deno_core::resolve_import;
use deno_core::FsModuleLoader;
use deno_core::JsRuntime;
use deno_core::ModuleLoader;
use deno_core::ModuleSource;
use deno_core::ModuleSourceFuture;
use deno_core::ModuleSpecifier;
use deno_core::ModuleType;
use deno_core::RuntimeOptions;
use eszip::format::Header;
use eszip::format::HeaderFrame;
use eszip::load_reqwest;
use futures::future::poll_fn;
use futures::task::Poll;
use std::collections::HashMap;
use std::marker::Unpin;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeekExt;
use tokio_util::codec::Framed;

pub struct StreamingLoader<R: AsyncReadExt + AsyncSeekExt + Unpin> {
  // TODO(@littledivy): Use `url::Url`
  headers: Arc<Mutex<HashMap<String, HeaderFrame>>>,
  buf: Arc<Mutex<R>>,
  header_size: Arc<Mutex<usize>>,
}

impl<R: AsyncReadExt + AsyncSeekExt + Unpin + 'static> ModuleLoader
  for StreamingLoader<R>
{
  fn resolve(
    &self,
    specifier: &str,
    referrer: &str,
    _is_main: bool,
  ) -> Result<ModuleSpecifier, Error> {
    Ok(resolve_import(specifier, referrer)?)
  }

  fn load(
    &self,
    module_specifier: &ModuleSpecifier,
    _maybe_referrer: Option<ModuleSpecifier>,
    _is_dyn_import: bool,
  ) -> Pin<Box<ModuleSourceFuture>> {
    let headers = Arc::clone(&self.headers);
    let buf = Arc::clone(&self.buf);
    let header_size = Arc::clone(&self.header_size);
    let specifier = module_specifier.to_string();
    async move {
      // FIXME: Can this be neater?
      let (frame, header_size) = poll_fn(|ctx| {
        match (
          headers.lock().unwrap().get(specifier.as_str()).cloned(),
          *header_size.lock().unwrap(),
        ) {
          (Some(frame), size) if size != 0 => Poll::Ready((frame, size)),
          (None, _) | (Some(_), 0) | _ => {
            // FIXME: Don't wake immediately
            let waker = ctx.waker().clone();
            waker.wake();
            Poll::Pending
          }
        }
      })
      .await;

      println!("{:?}", frame);
      let (url, start, size) = match frame {
        HeaderFrame::Module(specifier, source_ptr, ..) => {
          (specifier, source_ptr.0, source_ptr.1)
        }
        // FIXME: Maybe poll here for source here?
        HeaderFrame::Redirect(..) => unimplemented!(),
      };

      let mut buf = buf.lock().unwrap();
      // FIXME: Offset calculation hack.
      // Maybe offsets should be from the beginning of the file
      // instead of the data section?
      buf
        .seek(std::io::SeekFrom::Start(
          (8 + 32 + 4 + header_size + start) as u64,
        ))
        .await?;
      let mut source = vec![0u8; size];
      buf.read(&mut source).await?;

      let code = String::from_utf8_lossy(&source).to_string();
      println!("Source: {} HeaderSize: {}", code, header_size);
      Ok(ModuleSource {
        code,
        module_url_specified: specifier.to_string(),
        module_url_found: url,
        module_type: ModuleType::JavaScript,
      })
    }
    .boxed_local()
  }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
  let args: Vec<String> = std::env::args().collect();
  if args.len() < 2 {
    println!("Usage: target/examples/debug/loader <path_to_eszip>");
    std::process::exit(1);
  }
  let eszip = args[1].clone();

  let fd = tokio::fs::File::open(eszip).await?;

  let mut framed =
    Framed::new(fd.try_clone().await.unwrap(), Header::default());

  let headers = Arc::new(Mutex::new(HashMap::new()));
  let size = Arc::new(Mutex::new(0));
  let loader = StreamingLoader {
    headers: headers.clone(),
    buf: Arc::new(Mutex::new(fd)),
    header_size: size.clone(),
  };

  let mut js_runtime = JsRuntime::new(RuntimeOptions {
    module_loader: Some(Rc::new(loader)),
    ..Default::default()
  });

  let main_module = "file://main.js/".parse().unwrap();
  tokio::spawn(async move {
    let mut sent_size = false;
    while let Some(frame) = framed.next().await {
      if let Ok(frame) = frame {
        if !sent_size {
          *size.lock().unwrap() = framed.codec().header_size;
          sent_size = true;
        }
        let specifier = match frame {
          HeaderFrame::Module(ref specifier, ..) => specifier,
          HeaderFrame::Redirect(ref specifier, ..) => specifier,
        };

        headers.lock().unwrap().insert(specifier.to_string(), frame);
      }
    }
  });
  let mod_id = js_runtime.load_main_module(&main_module, None).await?;
  let _ = js_runtime.mod_evaluate(mod_id);
  js_runtime.run_event_loop(false).await?;
  Ok(())
}
