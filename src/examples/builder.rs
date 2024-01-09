// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

use std::collections::HashMap;
use std::sync::Arc;

use deno_ast::EmitOptions;
use deno_graph::source::CacheSetting;
use deno_graph::source::ResolveError;
use deno_graph::BuildOptions;
use deno_graph::CapturingModuleAnalyzer;
use deno_graph::GraphKind;
use deno_graph::ModuleGraph;
use import_map::ImportMap;
use reqwest::StatusCode;
use url::Url;

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let args = std::env::args().collect::<Vec<_>>();
  let url = args.get(1).unwrap();
  let url = Url::parse(url).unwrap();
  let out = args.get(2).unwrap();
  let maybe_import_map = args.get(3).map(|url| Url::parse(url).unwrap());

  let mut loader = Loader;
  let (maybe_import_map, maybe_import_map_data) =
    if let Some(import_map_url) = maybe_import_map {
      let resp = deno_graph::source::Loader::load(
        &mut loader,
        &import_map_url,
        false,
        CacheSetting::Use,
      )
      .await
      .unwrap()
      .unwrap();
      match resp {
        deno_graph::source::LoadResponse::Module {
          specifier, content, ..
        } => {
          let import_map =
            import_map::parse_from_json(&specifier, &content).unwrap();
          (Some(import_map.import_map), Some((specifier, content)))
        }
        _ => unimplemented!(),
      }
    } else {
      (None, None)
    };

  let analyzer = CapturingModuleAnalyzer::default();

  let mut graph = ModuleGraph::new(GraphKind::CodeOnly);
  graph
    .build(
      vec![url],
      &mut loader,
      BuildOptions {
        resolver: Some(&Resolver(maybe_import_map)),
        module_analyzer: Some(&analyzer),
        ..Default::default()
      },
    )
    .await;

  graph.valid().unwrap();

  let mut eszip = eszip::EszipV2::from_graph(
    graph,
    &analyzer.as_capturing_parser(),
    EmitOptions::default(),
  )
  .unwrap();
  if let Some((import_map_specifier, import_map_content)) =
    maybe_import_map_data
  {
    eszip.add_import_map(
      eszip::ModuleKind::Json,
      import_map_specifier.to_string(),
      Arc::from(import_map_content),
    )
  }
  for specifier in eszip.specifiers() {
    println!("source: {specifier}")
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
    referrer_range: &deno_graph::Range,
    _mode: deno_graph::source::ResolutionMode,
  ) -> Result<deno_graph::ModuleSpecifier, ResolveError> {
    if let Some(import_map) = &self.0 {
      import_map
        .resolve(specifier, &referrer_range.specifier)
        .map_err(|e| ResolveError::Other(e.into()))
    } else {
      Ok(deno_graph::resolve_import(
        specifier,
        &referrer_range.specifier,
      )?)
    }
  }
}

struct Loader;

impl deno_graph::source::Loader for Loader {
  fn load(
    &mut self,
    specifier: &deno_graph::ModuleSpecifier,
    _is_dynamic: bool,
    _cache_setting: CacheSetting,
  ) -> deno_graph::source::LoadFuture {
    let specifier = specifier.clone();

    Box::pin(async move {
      match specifier.scheme() {
        "data" => deno_graph::source::load_data_url(&specifier),
        "file" => {
          let path = std::fs::canonicalize(specifier.to_file_path().unwrap())?;
          let content = std::fs::read_to_string(&path)?;
          Ok(Some(deno_graph::source::LoadResponse::Module {
            specifier: Url::from_file_path(&path).unwrap(),
            maybe_headers: None,
            content: Arc::from(content),
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
            Ok(Some(deno_graph::source::LoadResponse::Module {
              specifier: url,
              maybe_headers: Some(headers),
              content: Arc::from(content),
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
