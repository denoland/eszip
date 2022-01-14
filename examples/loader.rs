use deno_core::anyhow::Error;
use deno_core::futures::FutureExt;
use deno_core::futures::StreamExt;
use deno_core::resolve_import;
use deno_core::FsModuleLoader;
use deno_core::JsRuntime;
use deno_core::ModuleLoader;
use deno_core::ModuleSourceFuture;
use deno_core::ModuleSpecifier;
use deno_core::RuntimeOptions;
use eszip::format::Header;
use eszip::format::HeaderFrame;
use eszip::load_reqwest;
use eszip::none_middleware;
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::sync::Mutex;
use tokio_util::codec::Framed;

pub struct StreamingLoader {
  headers: Arc<Mutex<HashMap<String, HeaderFrame>>>,
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
    let specifier = module_specifier.to_string();
    async move {
      let headers = headers.lock().await;
      let frame = headers.get(specifier.as_str()).unwrap();
      match frame {
        HeaderFrame::Module(..) => {}
        HeaderFrame::Redirect(..) => {}
      };

      unimplemented!()
    }
    .boxed_local()
  }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
  let args: Vec<String> = std::env::args().collect();
  if args.len() < 3 {
    println!(
      "Usage: target/examples/debug/loader <path_to_module> <path_to_eszip>"
    );
    std::process::exit(1);
  }
  let main_url = args[1].clone();
  println!("Run {}", main_url);
  let eszip = args[2].clone();

  let fd = tokio::fs::File::open(eszip).await?;

  let framed = Framed::new(fd, Header::default());
  let headers = Arc::new(Mutex::new(HashMap::new()));
  let loader = StreamingLoader {
    headers: headers.clone(),
  };

  let mut js_runtime = JsRuntime::new(RuntimeOptions {
    module_loader: Some(Rc::new(loader)),
    ..Default::default()
  });

  let main_module = deno_core::resolve_path(&main_url)?;

  framed
    .for_each(|frame| async {
      let frame = frame.unwrap();
      let specifier = match frame {
        HeaderFrame::Module(ref specifier, ..) => specifier,
        HeaderFrame::Redirect(ref specifier, ..) => specifier,
      };

      headers.lock().await.insert(specifier.to_string(), frame);
    })
    .await;
  let mod_id = js_runtime.load_main_module(&main_module, None).await?;
  let _ = js_runtime.mod_evaluate(mod_id);
  js_runtime.run_event_loop(false).await?;
  Ok(())
}
