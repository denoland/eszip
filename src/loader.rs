use crate::error::Error;
use crate::parser::get_deps_and_transpile;
use data_url::DataUrl;
use futures::stream::FuturesUnordered;
use futures::task::Poll;
use futures::Stream;
use futures::StreamExt;
use futures::TryFutureExt;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use url::Url;

pub trait ModuleLoader: Unpin {
  fn load(&self, url: Url) -> Pin<Box<ModuleLoadFuture>>;
}

// TODO(ry) Use ModuleSource instead? They're almost the same. Using ModuleSource would delegate
// the job of parsing dependencies and transpiling to the ModuleLoader implementer; whereas this is
// done generically at the moment.
pub enum ModuleLoad {
  Redirect(Url),
  Source {
    source: String,
    content_type: Option<String>,
  },
}

// Returns final url (after redirects) and source code.
pub type ModuleLoadFuture =
  dyn Send + Future<Output = Result<ModuleLoad, Error>>;

type ModuleInfoFuture =
  Pin<Box<dyn Send + Future<Output = Result<(Url, ModuleInfo), Error>>>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleSource {
  pub source: String,
  pub transpiled: Option<String>,
  pub content_type: Option<String>,
  pub deps: Vec<Url>,
}

impl ModuleSource {
  pub fn get_code(&self) -> String {
    self
      .transpiled
      .clone()
      .unwrap_or_else(|| self.source.clone())
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ModuleInfo {
  Redirect(Url),
  Source(ModuleSource),
}

pub struct ModuleStream<L: ModuleLoader> {
  started: HashSet<Url>,
  pending: FuturesUnordered<ModuleInfoFuture>,
  loader: L,
}

fn load_data_url(url: Url) -> Result<(Url, ModuleInfo), Error> {
  let data_url =
    DataUrl::process(url.as_str()).map_err(|e| Error::InvalidDataUrl {
      specifier: url.to_string(),
      error: format!("{:?}", e),
    })?;
  let (body, _) =
    data_url
      .decode_to_vec()
      .map_err(|e| Error::InvalidDataUrl {
        specifier: url.to_string(),
        error: format!("{:?}", e),
      })?;
  let source = String::from_utf8(body).map_err(|e| Error::InvalidDataUrl {
    specifier: url.to_string(),
    error: format!("{:?}", e),
  })?;
  let content_type = Some(data_url.mime_type().to_string());
  let (deps, transpiled) =
    get_deps_and_transpile(&url, &source, &content_type)?;
  Ok((
    url,
    ModuleInfo::Source(ModuleSource {
      source,
      content_type,
      deps,
      transpiled,
    }),
  ))
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
      if url.scheme() == "data" {
        self
          .pending
          .push(Box::pin(futures::future::ready(load_data_url(url))));
      } else if matches!(url.scheme(), "http" | "https") {
        let fut = Box::pin(self.loader.load(url.clone()).and_then(
          |module_source| async move {
            let module_info = match module_source {
              ModuleLoad::Redirect(url) => ModuleInfo::Redirect(url),
              ModuleLoad::Source {
                source,
                content_type,
              } => {
                let (deps, transpiled) =
                  get_deps_and_transpile(&url, &source, &content_type)?;
                ModuleInfo::Source(ModuleSource {
                  source,
                  transpiled,
                  content_type,
                  deps,
                })
              }
            };
            Ok((url, module_info))
          },
        ));
        self.pending.push(fut);
      } else {
        self.pending.push(Box::pin(futures::future::ready(Err(
          Error::InvalidScheme {
            scheme: url.scheme().to_string(),
            specifier: url.to_string(),
          },
        ))))
      }
    }
  }
}

impl<L: ModuleLoader> Stream for ModuleStream<L> {
  type Item = Result<(Url, ModuleInfo), Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    let r = self.pending.poll_next_unpin(cx);
    if let Poll::Ready(Some(Ok((ref _url, ref module_info)))) = r {
      match module_info {
        ModuleInfo::Redirect(url) => {
          self.append_module(url.clone());
        }
        ModuleInfo::Source(module_source) => {
          for dep in &module_source.deps {
            self.append_module(dep.clone());
          }
        }
      }
    }
    r
  }
}

/// Loader that loads from memory. Used for testing.
pub struct MemoryLoader(pub HashMap<Url, String>);

impl ModuleLoader for MemoryLoader {
  fn load(&self, specifier: Url) -> Pin<Box<ModuleLoadFuture>> {
    Box::pin(futures::future::ready(
      if let Some(source) = self.0.get(&specifier) {
        Ok(ModuleLoad::Source {
          source: source.clone(),
          content_type: None,
        })
      } else {
        Err(Error::NotFound {
          specifier: specifier.to_string(),
        })
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
    if let Poll::Ready(Some(Ok((url, module_info)))) = r {
      assert_eq!(url, root);
      if let ModuleInfo::Source(module_source) = module_info {
        assert_eq!(module_source.deps.len(), 1);
        assert!(module_source.source.contains("foo()"));
      } else {
        unreachable!()
      }
    } else {
      panic!("unexpected");
    }

    let r = Pin::new(&mut stream).poll_next(&mut cx);
    if let Poll::Ready(Some(Ok((url, module_info)))) = r {
      assert_eq!(url.as_str(), "http://deno.land/std/http/foo.ts");
      if let ModuleInfo::Source(module_source) = module_info {
        assert_eq!(module_source.deps.len(), 0);
        assert!(module_source.source.contains("console.log('hi')"));
      } else {
        unreachable!()
      }
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

  #[test]
  fn data_url() {
    let root =
      Url::parse("data:text/javascript;base64,Y29uc29sZS5sb2coJ2hpJyk7")
        .unwrap();
    let mut stream =
      ModuleStream::new(root.clone(), MemoryLoader(HashMap::new()));
    assert_eq!(stream.total(), 1);

    let mut cx =
      std::task::Context::from_waker(futures::task::noop_waker_ref());

    let r = Pin::new(&mut stream).poll_next(&mut cx);
    if let Poll::Ready(Some(Ok((url, module_info)))) = r {
      assert_eq!(
        url.as_str(),
        "data:text/javascript;base64,Y29uc29sZS5sb2coJ2hpJyk7"
      );
      if let ModuleInfo::Source(module_source) = module_info {
        assert_eq!(module_source.deps.len(), 0);
        assert!(module_source.source.contains("console.log('hi')"));
      } else {
        unreachable!()
      }
    } else {
      panic!("unexpected");
    }
  }

  #[test]
  fn error_on_invalid_scheme() {
    let root = Url::parse("file:///mod.ts").unwrap();
    let mut stream =
      ModuleStream::new(root.clone(), MemoryLoader(HashMap::new()));

    let mut cx =
      std::task::Context::from_waker(futures::task::noop_waker_ref());

    let r = Pin::new(&mut stream).poll_next(&mut cx);
    if let Poll::Ready(Some(Err(error))) = r {
      assert_eq!(
        error.to_string(),
        "scheme 'file' is not supported: 'file:///mod.ts'"
      )
    } else {
      panic!("unexpected");
    }
  }
}
