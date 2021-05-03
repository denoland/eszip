use crate::error::reqwest_error;
use crate::error::Error;
use crate::loader::ModuleLoad;
use crate::loader::ModuleLoadFuture;
use crate::loader::ModuleLoader;
use crate::loader::ModuleStream;
use crate::resolve_import::resolve_import;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::LOCATION;
use reqwest::RequestBuilder;
use std::pin::Pin;
use url::Url;

pub type ReqwestMiddlewareFn =
  Box<dyn Fn(RequestBuilder) -> RequestBuilder + Send + Sync>;

pub struct ReqwestLoader {
  client: reqwest::Client,
  middleware: Option<ReqwestMiddlewareFn>,
}

impl ModuleLoader for ReqwestLoader {
  fn load(&self, url: Url) -> Pin<Box<ModuleLoadFuture>> {
    let req = self.client.get(url.clone());
    let req = if let Some(ref middleware) = self.middleware {
      middleware(req)
    } else {
      req
    };
    Box::pin(async move {
      let res = req.send().await.map_err(|err| {
        if err.is_connect() || err.is_decode() {
          Error::Download {
            specifier: url.to_string(),
            inner: err,
          }
        } else {
          Error::Other(Box::new(err))
        }
      })?;

      if res.status().is_redirection() {
        let location = res
          .headers()
          .get(LOCATION)
          .ok_or_else(|| Error::InvalidRedirect {
            specifier: url.to_string(),
          })?
          .to_str()
          .map_err(|_| Error::InvalidRedirect {
            specifier: url.to_string(),
          })?;
        let location_resolved = resolve_import(&location, url.as_str())?;
        Ok(ModuleLoad::Redirect(location_resolved))
      } else if res.status().is_success() {
        let content_type = res
          .headers()
          .get(CONTENT_TYPE)
          .map(|v| v.to_str().unwrap_or_default().to_string());
        let source = res
          .text()
          .await
          .map_err(|err| reqwest_error(url.to_string(), err))?;
        Ok(ModuleLoad::Source {
          source,
          content_type,
        })
      } else {
        Err(reqwest_error(
          url.to_string(),
          res.error_for_status().unwrap_err(),
        ))
      }
    })
  }
}

/// Loads modules over HTTP using reqwest
pub fn load_reqwest(
  root: Url,
  client_builder: reqwest::ClientBuilder,
  middleware: Option<ReqwestMiddlewareFn>,
) -> ModuleStream<ReqwestLoader> {
  let client = client_builder
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .unwrap();
  ModuleStream::new(root, ReqwestLoader { client, middleware })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::loader::ModuleInfo;

  #[test]
  fn stream_is_send() {
    fn is_send<T: Send>() {}
    is_send::<ModuleStream<ReqwestLoader>>();
  }

  // Requires internet access!
  #[tokio::test]
  async fn load_003_relative_import() {
    let root = Url::parse(
    "https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/003_relative_import.ts",
  )
  .unwrap();

    let module_stream =
      load_reqwest(root.clone(), reqwest::ClientBuilder::new(), None);

    use futures::stream::TryStreamExt;
    let modules: Vec<(Url, ModuleInfo)> =
      module_stream.try_collect().await.unwrap();

    assert_eq!(modules.len(), 2);

    let (_url, root_info) = &modules[0];
    if let ModuleInfo::Source(module_source) = root_info {
      assert_eq!(module_source.deps.len(), 1);
      assert!(module_source.source.contains("printHello"));
    } else {
      unreachable!()
    }

    let (url, print_hello_info) = &modules[1];
    assert_eq!(url.as_str(),
    "https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/subdir/print_hello.ts");
    if let ModuleInfo::Source(module_source) = print_hello_info {
      assert_eq!(module_source.deps.len(), 0);
      assert!(module_source.source.contains("function printHello(): void"));
    } else {
      unreachable!()
    }
  }

  // Requires internet access!
  #[tokio::test]
  async fn redirect() {
    let root = Url::parse(
    "https://gist.githubusercontent.com/satyarohith/76affa966ff919369dd74421749863c2/raw/dcfae001eb1f1c8a350080f4e2e3a8e09e3ab4ce/redirect_example.ts",
  )
  .unwrap();

    let module_stream =
      load_reqwest(root.clone(), reqwest::ClientBuilder::new(), None);

    use futures::stream::TryStreamExt;
    let modules: Vec<(Url, ModuleInfo)> =
      module_stream.try_collect().await.unwrap();

    assert_eq!(modules.len(), 7);
  }

  // Requires internet access!
  #[tokio::test]
  async fn middleware() {
    let root = Url::parse("https://eszip-tests.deno.dev/").unwrap();

    fn middleware(builder: RequestBuilder) -> RequestBuilder {
      builder.header("x-magic-auth", "foobar")
    }

    let module_stream = load_reqwest(
      root.clone(),
      reqwest::ClientBuilder::new(),
      Some(Box::new(middleware)),
    );

    use futures::stream::TryStreamExt;
    let modules: Vec<(Url, ModuleInfo)> =
      module_stream.try_collect().await.unwrap();
    assert_eq!(modules.len(), 1);

    let (_url, info) = modules.get(0).unwrap();
    let src = match info {
      ModuleInfo::Source(src) => src,
      _ => unreachable!(),
    };
    assert_eq!(src.source, r#""foobar""#)
  }
}
