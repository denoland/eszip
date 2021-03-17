use crate::loader::ModuleLoad;
use crate::loader::ModuleLoadFuture;
use crate::loader::ModuleLoader;
use crate::loader::ModuleStream;
use crate::resolve_import::resolve_import;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::LOCATION;
use std::pin::Pin;
use url::Url;

pub struct ReqwestLoader(reqwest::Client);

impl ModuleLoader for ReqwestLoader {
  fn load(&self, url: Url) -> Pin<Box<ModuleLoadFuture>> {
    let client = self.0.clone();
    Box::pin(async move {
      let res = client.get(url.clone()).send().await?;

      if res.status().is_redirection() {
        let location = res.headers().get(LOCATION).unwrap().to_str().unwrap();
        let location_resolved = resolve_import(&location, url.as_str())?;
        Ok(ModuleLoad::Redirect(location_resolved))
      } else if res.status().is_success() {
        let content_type = res
          .headers()
          .get(CONTENT_TYPE)
          .map(|v| v.to_str().unwrap().to_string());
        let source = res.text().await?;
        Ok(ModuleLoad::Source {
          source,
          content_type,
        })
      } else {
        res.error_for_status()?;
        unreachable!()
      }

      // Ok((final_url, source))
    })
  }
}

/// Loads modules over HTTP using reqwest
pub fn load_reqwest(
  root: Url,
  client_builder: reqwest::ClientBuilder,
) -> ModuleStream<ReqwestLoader> {
  let client = client_builder
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .unwrap();
  ModuleStream::new(root, ReqwestLoader(client))
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
      load_reqwest(root.clone(), reqwest::ClientBuilder::new());

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
      load_reqwest(root.clone(), reqwest::ClientBuilder::new());

    use futures::stream::TryStreamExt;
    let modules: Vec<(Url, ModuleInfo)> =
      module_stream.try_collect().await.unwrap();

    assert_eq!(modules.len(), 7);
  }
}
