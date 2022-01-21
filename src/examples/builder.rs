use std::collections::HashMap;
use std::sync::Arc;

use import_map::ImportMap;
use reqwest::StatusCode;
use url::Url;

#[tokio::main]
async fn main() {
  let args = std::env::args().collect::<Vec<_>>();
  let url = args.get(1).unwrap();
  let url = Url::parse(url).unwrap();
  let out = args.get(2).unwrap();
  let maybe_import_map = args.get(3).map(|url| Url::parse(url).unwrap());

  let mut loader = Loader;
  let (maybe_import_map, maybe_import_map_data) =
    if let Some(import_map_url) = maybe_import_map {
      let resp =
        deno_graph::source::Loader::load(&mut loader, &import_map_url, false)
          .await
          .unwrap()
          .unwrap();
      let import_map =
        ImportMap::from_json_with_diagnostics(&resp.specifier, &resp.content)
          .unwrap();
      (
        Some(import_map.import_map),
        Some((resp.specifier, resp.content)),
      )
    } else {
      (None, None)
    };

  let graph = deno_graph::create_code_graph(
    vec![url],
    false,
    None,
    &mut loader,
    Some(&Resolver(maybe_import_map)),
    None,
    None,
    None,
  )
  .await;

  graph.valid().unwrap();

  let mut eszip = eszip::EsZipV2::from_graph(graph).unwrap();
  if let Some((import_map_specifier, import_map_content)) =
    maybe_import_map_data
  {
    eszip.insert_module(
      import_map_specifier.to_string(),
      eszip::ModuleKind::Json,
      Arc::new(import_map_content.as_bytes().to_vec()),
    )
  }
  let bytes = eszip.into_bytes();

  std::fs::write(out, bytes).unwrap();
}

#[derive(Debug)]
struct Resolver(Option<ImportMap>);

impl deno_graph::source::Resolver for Resolver {
  fn resolve(
    &self,
    specifier: &str,
    referrer: &deno_graph::ModuleSpecifier,
  ) -> anyhow::Result<deno_graph::ModuleSpecifier> {
    let resolved = if let Some(import_map) = &self.0 {
      import_map.resolve(specifier, referrer)?
    } else {
      deno_graph::resolve_import(specifier, referrer)?
    };
    Ok(resolved)
  }
}

struct Loader;

impl deno_graph::source::Loader for Loader {
  fn load(
    &mut self,
    specifier: &deno_graph::ModuleSpecifier,
    is_dynamic: bool,
  ) -> deno_graph::source::LoadFuture {
    let specifier = specifier.clone();

    Box::pin(async move {
      if is_dynamic {
        return Ok(None);
      }

      match specifier.scheme() {
        "data" => deno_graph::source::load_data_url(&specifier),
        "file" => {
          let path =
            tokio::fs::canonicalize(specifier.to_file_path().unwrap()).await?;
          let content = tokio::fs::read(&path).await?;
          let content = String::from_utf8(content)?;
          Ok(Some(deno_graph::source::LoadResponse {
            specifier: Url::from_file_path(&path).unwrap(),
            maybe_headers: None,
            content: Arc::new(content),
          }))
        }
        "http" | "https" => {
          let resp = reqwest::get(specifier.as_str()).await?;
          if resp.status() == StatusCode::NOT_FOUND {
            Ok(None)
          } else {
            let resp = resp.error_for_status()?;
            let mut headers = HashMap::new();
            for key in resp.headers().keys() {
              let key_str = key.to_string();
              let values = resp.headers().get_all(key);
              let values_str = values
                .iter()
                .filter_map(|e| e.to_str().ok())
                .collect::<Vec<&str>>()
                .join(",");
              headers.insert(key_str, values_str);
            }
            let url = resp.url().clone();
            let content = resp.text().await?;
            Ok(Some(deno_graph::source::LoadResponse {
              specifier: url,
              maybe_headers: Some(headers),
              content: Arc::new(content),
            }))
          }
        }
        _ => Err(anyhow::anyhow!(
          "unsupported scheme: {}",
          specifier.scheme()
        )),
      }
    })
  }
}
