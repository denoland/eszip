use crate::parse_deps::parse_deps;
use anyhow::Error;
use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use futures::task::Poll;
use futures::Stream;
use futures::StreamExt;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use url::Url;

#[async_trait]
pub trait ModuleLoader: Unpin {
  async fn load(url: Url) -> Result<String, Error>;
}

type ModuleInfoFuture =
  Pin<Box<dyn Future<Output = Result<ModuleInfo, Error>>>>;

#[derive(Clone)]
pub struct ModuleInfo {
  pub url: Url,
  pub deps: Vec<Url>,
  pub source: String,
}

pub struct ModuleStream<L: ModuleLoader> {
  started: HashSet<Url>,
  pending: FuturesUnordered<ModuleInfoFuture>,
  _data: std::marker::PhantomData<L>,
}

impl<L: ModuleLoader> Stream for ModuleStream<L> {
  type Item = Result<ModuleInfo, Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    let r = self.pending.poll_next_unpin(cx);
    if let Poll::Ready(Some(Ok(ref module_info))) = r {
      for dep in &module_info.deps {
        self.append_module(dep.clone());
      }
    }
    r
  }
}

impl<L: ModuleLoader> ModuleStream<L> {
  pub fn new(root: Url) -> Self {
    let mut g = Self {
      started: HashSet::new(),
      pending: FuturesUnordered::new(),
      _data: Default::default(),
    };
    g.append_module(root);
    g
  }

  pub fn total(&self) -> usize {
    self.started.len()
  }

  fn append_module(&mut self, url: Url) {
    if !self.started.contains(&url) {
      self.started.insert(url.clone());
      let url_ = url.clone();
      use futures::TryFutureExt;
      let fut = L::load(url).and_then(|source| async move {
        let deps = parse_deps(&url_, &source)?;
        Ok(ModuleInfo {
          url: url_,
          source: source.to_string(),
          deps,
        })
      });
      self.pending.push(Box::pin(fut));
    }
  }
}

pub struct ReqwestLoader;

#[async_trait]
impl ModuleLoader for ReqwestLoader {
  async fn load(url: Url) -> Result<String, Error> {
    let source = reqwest::get(url).await?.text().await?;
    Ok(source)
  }
}

/// Loads modules over HTTP using reqwest
pub fn load_reqwest(root: Url) -> ModuleStream<ReqwestLoader> {
  ModuleStream::new(root)
}

// Requires internet access!
#[tokio::test]
async fn load_003_relative_import() {
  let root = Url::parse(
    "https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/003_relative_import.ts",
  )
  .unwrap();

  let module_stream = load_reqwest(root.clone());

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
