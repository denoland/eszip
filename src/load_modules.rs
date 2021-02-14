use crate::parse_deps::parse_deps;
use anyhow::Error;
use futures::stream::FuturesUnordered;
use futures::task::Poll;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use url::Url;

type Graph = HashMap<Url, ModuleInfo>;
type DepsFuture = Pin<Box<dyn Future<Output = Result<Vec<Url>, Error>>>>;

pub async fn load_modules(root: Url) -> Result<Graph, Error> {
  let g = ModuleGraphFuture::new(root);
  g.await
}

struct ModuleGraphFuture {
  loaded: Arc<Mutex<Option<Graph>>>,
  pending: FuturesUnordered<DepsFuture>,
}

pub struct ModuleInfo {
  pub source: String,
  pub deps: Vec<Url>,
}

impl ModuleGraphFuture {
  pub fn new(root: Url) -> Self {
    let mut g = Self {
      loaded: Arc::new(Mutex::new(Some(HashMap::new()))),
      pending: FuturesUnordered::new(),
    };
    g.append_module(root);
    g
  }

  fn append_module(&mut self, url: Url) {
    if !self.already_loaded(&url) {
      let loaded = self.loaded.clone();
      self.pending.push(Box::pin(async move {
        let module_info = fetch(&url).await?;
        let mut l = loaded.lock().unwrap();
        let deps = module_info.deps.clone();
        l.as_mut().unwrap().insert(url, module_info);
        Ok(deps)
      }));
    }
  }

  fn already_loaded(&self, url: &Url) -> bool {
    let loaded = self.loaded.lock().unwrap();
    loaded.as_ref().unwrap().contains_key(url)
  }
}

impl Future for ModuleGraphFuture {
  type Output = Result<Graph, anyhow::Error>;

  fn poll(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Self::Output> {
    use futures::StreamExt;
    match self.pending.poll_next_unpin(cx) {
      Poll::Ready(None) => {
        let mut l = self.loaded.lock().unwrap();
        Poll::Ready(Ok(l.take().unwrap()))
      }
      Poll::Pending => Poll::Pending,
      Poll::Ready(Some(Ok(deps))) => {
        for dep in deps.into_iter() {
          self.append_module(dep);
        }
        cx.waker().wake_by_ref();
        Poll::Pending
      }
      Poll::Ready(Some(Err(e))) => Poll::Ready(Err(e.into())),
    }
  }
}

async fn fetch(url: &Url) -> Result<ModuleInfo, Error> {
  let source = reqwest::get(url.clone()).await?.text().await?;
  let deps = parse_deps(url, &source)?;
  Ok(ModuleInfo { source, deps })
}

// Requires internet access!
#[tokio::test]
async fn load_003_relative_import() {
  let root = Url::parse(
    "https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/003_relative_import.ts",
  )
  .unwrap();

  let modules = load_modules(root.clone()).await.unwrap();

  let root_info = modules.get(&root).unwrap();
  assert_eq!(root_info.deps.len(), 1);
  assert!(root_info.source.contains("printHello"));

  let print_hello = Url::parse("https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/subdir/print_hello.ts").unwrap();
  let print_hello_info = modules.get(&print_hello).unwrap();
  assert_eq!(print_hello_info.deps.len(), 0);
  assert!(print_hello_info
    .source
    .contains("function printHello(): void"));
}
