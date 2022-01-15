use deno_core::anyhow::Error;
use deno_core::futures::FutureExt;
use deno_core::futures::StreamExt;
use deno_core::resolve_import;
use deno_core::JsRuntime;
use deno_core::ModuleLoader;
use deno_core::ModuleSource;
use deno_core::ModuleSourceFuture;
use deno_core::ModuleSpecifier;
use deno_core::ModuleType;
use deno_core::RuntimeOptions;
use eszip::format::Header;
use eszip::format::HeaderFrame;
use eszip::format::ModuleKind;
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeekExt;
use tokio::sync::oneshot;
use tokio_util::codec::Framed;
use url::Url;

pub struct StreamingLoader {
  headers: Arc<Mutex<HashMap<Url, SourceSlot>>>,
}

enum SourceSlot {
  Ready(Source),
  Needed(oneshot::Sender<()>),
  Pending,
}

enum Source {
  Module { kind: ModuleKind, source: Vec<u8> },
  Redirect(Url),
}

impl ModuleLoader for StreamingLoader {
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
    let specifier = module_specifier.to_owned();
    async move {
      let mut sources = headers.lock().unwrap();
      println!("load: {}", specifier);

      let new_specifier = match sources.get(&specifier) {
        None | Some(SourceSlot::Pending) => {
          let (tx, rx) = oneshot::channel();
          sources.insert(specifier.clone(), SourceSlot::Needed(tx));
          // Drops the lock for the sender.
          // Important otherwise it's a deadlock.
          drop(sources);
          rx.await.unwrap();
          specifier.clone()
        }
        Some(SourceSlot::Ready(Source::Redirect(redirect))) => {
          println!("redirect: {}", redirect);
          let redirect = redirect.clone();
          match sources.get(&redirect) {
            None | Some(SourceSlot::Pending) => {
              let (tx, rx) = oneshot::channel();
              sources.insert(redirect.clone(), SourceSlot::Needed(tx));
              // Drops the lock for the sender.
              // Important otherwise it's a deadlock.
              drop(sources);
              rx.await.unwrap();
            }
            _ => drop(sources),
          }

          redirect
        }
        _ => specifier.clone(),
      };

      // Re-acquire the lock.
      let sources = headers.lock().unwrap();
      let slot = sources.get(&new_specifier).unwrap();
      let source = match slot {
        SourceSlot::Ready(Source::Module { source, .. }) => source,
        SourceSlot::Needed(_) | SourceSlot::Pending | _ => {
          unreachable!()
        }
      };
      let code = String::from_utf8_lossy(source).to_string();
      println!("code: {}", code);
      Ok(ModuleSource {
        code,
        module_url_specified: specifier.to_string(),
        module_url_found: new_specifier.to_string(),
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

  let mut fd = tokio::fs::File::open(eszip).await?;

  let mut framed =
    Framed::new(fd.try_clone().await.unwrap(), Header::default());

  let headers = Arc::new(Mutex::new(HashMap::new()));
  let loader = StreamingLoader {
    headers: Arc::clone(&headers),
  };

  let mut js_runtime = JsRuntime::new(RuntimeOptions {
    module_loader: Some(Rc::new(loader)),
    ..Default::default()
  });

  let main_module = "file://main.js/".parse().unwrap();
  tokio::spawn(async move {
    while let Some(frame) = framed.next().await {
      if let Ok(frame) = frame {
        let (specifier, start, size, kind) = match frame {
          HeaderFrame::Module(ref specifier, ptr, _, kind) => {
            (specifier, ptr.offset(), ptr.size(), kind)
          }
          HeaderFrame::Redirect(ref specifier, ref redirect) => {
            println!("redirect: {} -> {}", specifier, redirect);
            let mut headers = headers.lock().unwrap();
            headers.insert(
              Url::parse(specifier).unwrap(),
              SourceSlot::Ready(Source::Redirect(
                Url::parse(redirect).unwrap(),
              )),
            );
            drop(headers);
            continue;
          }
        };

        let start = framed.codec().header_size() + start;
        fd.seek(std::io::SeekFrom::Start(start as u64))
          .await
          .unwrap();
        let mut source = vec![0; size];
        fd.read_exact(&mut source).await.unwrap();

        println!("send: {}", specifier);
        match headers.lock().unwrap().insert(
          Url::parse(specifier).unwrap(),
          SourceSlot::Ready(Source::Module { kind, source }),
        ) {
          // module loader is waiting for this module.
          // let it know it's ready.
          Some(SourceSlot::Needed(tx)) => tx.send(()).unwrap(),
          Some(SourceSlot::Pending) => {
            println!("pending: {}", specifier);
          }
          _ => {}
        };
      }
    }
  });

  let mod_id = js_runtime.load_main_module(&main_module, None).await?;
  let _ = js_runtime.mod_evaluate(mod_id);
  println!("done");

  js_runtime.run_event_loop(false).await?;
  Ok(())
}
