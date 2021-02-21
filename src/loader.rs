use crate::parse_deps::parse_deps;
use anyhow::anyhow;
use anyhow::Error;
use futures::stream::FuturesUnordered;
use futures::task::Poll;
use futures::Stream;
use futures::StreamExt;
use futures::TryFutureExt;
use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use url::Url;

pub trait ModuleLoader: Unpin {
  fn load(&self, url: Url) -> Pin<Box<ModuleSourceFuture>>;
}

pub type ModuleSourceFuture = dyn Send + Future<Output = Result<String, Error>>;

type ModuleInfoFuture =
  Pin<Box<dyn Send + Future<Output = Result<ModuleInfo, Error>>>>;

#[derive(Clone, Debug)]
pub struct ModuleInfo {
  pub url: Url,
  pub deps: Vec<Url>,
  pub source: String,
}

pub struct ModuleStream<L: ModuleLoader> {
  started: HashSet<Url>,
  pending: FuturesUnordered<ModuleInfoFuture>,
  loader: L,
}

impl<L: ModuleLoader> ModuleStream<L> {
  pub fn new(root: Url, loader: L) -> Self {
    let mut g = Self {
      started: HashSet::new(),
      pending: FuturesUnordered::new(),
      loader,
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
      let fut = Box::pin(self.loader.load(url).and_then(|source| async move {
        let deps = parse_deps(&url_, &source)?;
        Ok(ModuleInfo {
          url: url_,
          source: source.to_string(),
          deps,
        })
      }));
      self.pending.push(fut);
    }
  }
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

/// Loader that loads from memory. Used for testing.
pub struct MemoryLoader(pub HashMap<Url, String>);

impl ModuleLoader for MemoryLoader {
  fn load(&self, url: Url) -> Pin<Box<ModuleSourceFuture>> {
    Box::pin(futures::future::ready(
      if let Some(source) = self.0.get(&url) {
        Ok(source.clone())
      } else {
        Err(anyhow!("not found"))
      },
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn basic() {
    let root = Url::parse("http://deno.land/std/http/server.ts").unwrap();
    let mut hm = HashMap::new();
    hm.insert(
      root.clone(),
      r#"import { foo } from "./foo.ts"; foo();"#.to_string(),
    );
    hm.insert(
      Url::parse("http://deno.land/std/http/foo.ts").unwrap(),
      r#"console.log('hi')"#.to_string(),
    );

    let mut stream = ModuleStream::new(root.clone(), MemoryLoader(hm));
    assert_eq!(stream.total(), 1);

    let mut cx =
      std::task::Context::from_waker(futures::task::noop_waker_ref());

    let r = Pin::new(&mut stream).poll_next(&mut cx);
    if let Poll::Ready(Some(Ok(module_info))) = r {
      assert_eq!(module_info.url, root);
      assert_eq!(module_info.deps.len(), 1);
      assert!(module_info.source.contains("foo()"));
    } else {
      panic!("unexpected");
    }

    let r = Pin::new(&mut stream).poll_next(&mut cx);
    if let Poll::Ready(Some(Ok(module_info))) = r {
      assert_eq!(module_info.url.as_str(), "http://deno.land/std/http/foo.ts");
      assert_eq!(module_info.deps.len(), 0);
      assert!(module_info.source.contains("console.log('hi')"));
    } else {
      panic!("unexpected");
    }

    let r = Pin::new(&mut stream).poll_next(&mut cx);
    if let Poll::Ready(None) = r {
      // expected
    } else {
      panic!("unexpected");
    }
  }
}
