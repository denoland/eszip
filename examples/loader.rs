use deno_core::anyhow::Error;
use deno_core::futures::FutureExt;
use deno_core::resolve_import;
use deno_core::FsModuleLoader;
use deno_core::JsRuntime;
use deno_core::ModuleLoader;
use deno_core::ModuleSourceFuture;
use deno_core::ModuleSpecifier;
use deno_core::RuntimeOptions;
use eszip::format::Header;
use eszip::load_reqwest;
use eszip::none_middleware;
use std::pin::Pin;
use std::rc::Rc;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio_util::codec::Framed;

pub struct StreamingLoader<T: AsyncRead + AsyncWrite> {
  header: Framed<T, Header>,
}

impl<T: AsyncRead + AsyncWrite> ModuleLoader for StreamingLoader<T> {
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
    _module_specifier: &ModuleSpecifier,
    _maybe_referrer: Option<ModuleSpecifier>,
    _is_dyn_import: bool,
  ) -> Pin<Box<ModuleSourceFuture>> {
    async { unimplemented!() }.boxed_local()
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

  let loader = StreamingLoader {
    header: Framed::new(fd, Header::default()),
  };

  let mut js_runtime = JsRuntime::new(RuntimeOptions {
    module_loader: Some(Rc::new(loader)),
    ..Default::default()
  });

  let main_module = deno_core::resolve_path(&main_url)?;

  let mod_id = js_runtime.load_main_module(&main_module, None).await?;
  let _ = js_runtime.mod_evaluate(mod_id);
  js_runtime.run_event_loop(false).await?;
  Ok(())
}
