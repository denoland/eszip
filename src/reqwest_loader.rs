use crate::loader::ModuleLoader;
use crate::loader::ModuleSourceFuture;
use crate::loader::ModuleStream;
use futures::FutureExt;
use std::pin::Pin;
use url::Url;

pub struct ReqwestLoader;

impl ModuleLoader for ReqwestLoader {
  fn load(&self, url: Url) -> Pin<Box<ModuleSourceFuture>> {
    async move {
      let source = reqwest::get(url).await?.text().await?;
      Ok(source)
    }
    .boxed_local()
  }
}

/// Loads modules over HTTP using reqwest
pub fn load_reqwest(root: Url) -> ModuleStream<ReqwestLoader> {
  ModuleStream::new(root, ReqwestLoader)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::loader::ModuleInfo;

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
}
