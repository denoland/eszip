use std::collections::HashMap;
use std::sync::Arc;

use reqwest::StatusCode;
use url::Url;

#[tokio::main]
async fn main() {
  let args = std::env::args().collect::<Vec<_>>();
  let url = args.get(1).unwrap();
  let url = Url::parse(&url).unwrap();
  let out = args.get(2).unwrap();

  let mut loader = Loader;
  let graph = deno_graph::create_code_graph(
    vec![url],
    false,
    None,
    &mut loader,
    None,
    None,
    None,
    None,
  )
  .await;

  graph.valid().unwrap();

  let eszip = eszip::EsZipV2::from_graph(graph).unwrap();
  let bytes = eszip.into_bytes();

  std::fs::write(out, bytes).unwrap();
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
