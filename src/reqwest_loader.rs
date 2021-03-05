use crate::loader::ModuleLoader;
use crate::loader::ModuleSourceFuture;
use crate::loader::ModuleStream;
use std::pin::Pin;
use url::Url;

pub struct ReqwestLoader(reqwest::Client);

impl ModuleLoader for ReqwestLoader {
  fn load(&self, url: Url) -> Pin<Box<ModuleSourceFuture>> {
    let client = self.0.clone();
    Box::pin(async move {
      let res = client.get(url.clone()).send().await?;
      let final_url = res.url().clone();
      let source = res.error_for_status()?.text().await?;
      Ok((final_url, source))
    })
  }
}

/// Loads modules over HTTP using reqwest
pub fn load_reqwest(
  root: Url,
  client: reqwest::Client,
) -> ModuleStream<ReqwestLoader> {
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

    let module_stream = load_reqwest(root.clone(), reqwest::Client::new());

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

  // Requires internet access!
  #[tokio::test]
  async fn redirect() {
    let root = Url::parse(
    "https://gist.githubusercontent.com/satyarohith/76affa966ff919369dd74421749863c2/raw/dcfae001eb1f1c8a350080f4e2e3a8e09e3ab4ce/redirect_example.ts",
  )
  .unwrap();

    let module_stream = load_reqwest(root.clone(), reqwest::Client::new());

    use futures::stream::TryStreamExt;
    let modules: Vec<ModuleInfo> = module_stream.try_collect().await.unwrap();

    assert_eq!(modules.len(), 6);
  }
}
