use crate::parse_deps::parse_deps;
use anyhow::Error;
use futures::stream::FuturesUnordered;
use futures::task::Poll;
use futures::Stream;
use futures::StreamExt;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use url::Url;

type ModuleInfoFuture =
  Pin<Box<dyn Future<Output = Result<ModuleInfo, Error>>>>;

pub fn load_modules(root: Url) -> ModuleStream {
  ModuleStream::new(root)
}

#[derive(Clone)]
pub struct ModuleInfo {
  pub url: Url,
  pub deps: Vec<Url>,
  pub source: String,
}

pub struct ModuleStream {
  started: HashSet<Url>,
  pending: FuturesUnordered<ModuleInfoFuture>,
  pub total: usize,
}

impl Stream for ModuleStream {
  type Item = Result<ModuleInfo, Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    match self.pending.poll_next_unpin(cx) {
      Poll::Ready(Some(Ok(module_info))) => {
        for dep in &module_info.deps {
          self.append_module(dep.clone());
        }
        Poll::Ready(Some(Ok(module_info)))
      }
      x => x,
    }
  }
}

impl ModuleStream {
  pub fn new(root: Url) -> Self {
    let mut g = Self {
      started: HashSet::new(),
      pending: FuturesUnordered::new(),
      total: 0,
    };
    g.append_module(root);
    g
  }

  fn append_module(&mut self, url: Url) {
    if !self.started.contains(&url) {
      self.started.insert(url.clone());
      self.total += 1;
      self.pending.push(Box::pin(async move {
        let module_info = fetch(&url).await?;
        Ok(module_info)
      }));
    }
  }
}

async fn fetch(url: &Url) -> Result<ModuleInfo, Error> {
  let source = reqwest::get(url.clone()).await?.text().await?;
  let deps = parse_deps(url, &source)?;
  Ok(ModuleInfo {
    url: url.clone(),
    source,
    deps,
  })
}

// Requires internet access!
#[tokio::test]
async fn load_003_relative_import() {
  let root = Url::parse(
    "https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/003_relative_import.ts",
  )
  .unwrap();

  let module_stream = load_modules(root.clone());

  use futures::stream::TryStreamExt;
  let modules: Vec<ModuleInfo> = module_stream.try_collect().await.unwrap();

  assert_eq!(modules.len(), 2);

  let root_info = &modules[0];
  assert_eq!(root_info.deps.len(), 1);
  assert!(root_info.source.contains("printHello"));

  let print_hello_info = &modules[1];
  assert_eq!(print_hello_info.deps.len(), 0);
  assert_eq!(print_hello_info.url.as_str(),
    "https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/subdir/print_hello.ts");
  assert!(print_hello_info
    .source
    .contains("function printHello(): void"));
}
