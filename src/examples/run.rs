use std::rc::Rc;

use deno_core::error::type_error;
use eszip::EsZipV2;
use futures::FutureExt;
use import_map::ImportMap;
use url::Url;

#[tokio::main]
async fn main() {
  let args = std::env::args().collect::<Vec<_>>();
  let path = args.get(1).unwrap();
  let url = args.get(2).unwrap();
  let url = Url::parse(url).unwrap();
  let maybe_import_map = args.get(3).map(|url| Url::parse(url).unwrap());

  let file = tokio::fs::File::open(path).await.unwrap();
  let bufreader = tokio::io::BufReader::new(file);
  let (eszip, loader) = eszip::EsZipV2::parse(bufreader).await.unwrap();

  let loader_fut = loader.map(|r| r.map_err(anyhow::Error::new));

  let fut = async move {
    let maybe_import_map = if let Some(maybe_import_map) = maybe_import_map {
      let module = eszip.get_module(maybe_import_map.as_str()).unwrap();
      let source = module.source().await;
      let contents = std::str::from_utf8(&source).unwrap();
      let import_map =
        ImportMap::from_json_with_diagnostics(&maybe_import_map, contents)
          .unwrap();
      Some(import_map.import_map)
    } else {
      None
    };

    let mut runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
      module_loader: Some(Rc::new(Loader(eszip, maybe_import_map))),
      extensions: vec![deno_console::init()],
      ..Default::default()
    });

    let mod_id = runtime.load_main_module(&url, None).await?;

    let fut = runtime
      .mod_evaluate(mod_id)
      .map(|r| r.map_err(anyhow::Error::new));

    let (_, r) = tokio::try_join!(runtime.run_event_loop(false), fut)?;

    r
  };

  tokio::try_join!(loader_fut, fut).unwrap();
}

struct Loader(EsZipV2, Option<ImportMap>);

impl deno_core::ModuleLoader for Loader {
  fn resolve(
    &self,
    specifier: &str,
    base: &str,
    is_main: bool,
  ) -> Result<deno_core::ModuleSpecifier, anyhow::Error> {
    if let Some(import_map) = &self.1 {
      let referrer = if base == "." {
        assert!(is_main);
        Url::parse("file:///src/").unwrap()
      } else {
        Url::parse(base).unwrap()
      };
      let resolved = import_map.resolve(specifier, &referrer)?;
      Ok(resolved)
    } else {
      let resolve = deno_core::resolve_import(specifier, base)?;
      Ok(resolve)
    }
  }

  fn load(
    &self,
    module_specifier: &deno_core::ModuleSpecifier,
    _maybe_referrer: Option<deno_core::ModuleSpecifier>,
    is_dyn_import: bool,
  ) -> std::pin::Pin<Box<deno_core::ModuleSourceFuture>> {
    let module_specifier = module_specifier.clone();

    let res = self
      .0
      .get_module(module_specifier.as_str())
      .ok_or_else(|| type_error("module not found"));

    Box::pin(async move {
      if is_dyn_import {
        return Err(type_error("dynamic import not supported"));
      }

      let module = res?;

      let source = module.source().await;
      let source = std::str::from_utf8(&source).unwrap();

      Ok(deno_core::ModuleSource {
        code: source.to_string(),
        module_type: match module.kind {
          eszip::ModuleKind::JavaScript => deno_core::ModuleType::JavaScript,
          eszip::ModuleKind::Json => deno_core::ModuleType::Json,
        },
        module_url_found: module.specifier,
        module_url_specified: module_specifier.to_string(),
      })
    })
  }
}
