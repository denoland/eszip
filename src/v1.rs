use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use url::Url;

use crate::Module;
use crate::ModuleInner;
use crate::ModuleKind;
use crate::ParseError;

pub const ESZIP_V1_GRAPH_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EszipV1 {
  version: u32,
  modules: HashMap<Url, ModuleInfo>,
}

impl EszipV1 {
  pub fn from_modules(modules: HashMap<Url, ModuleInfo>) -> Self {
    Self {
      version: ESZIP_V1_GRAPH_VERSION,
      modules,
    }
  }

  pub fn parse(data: &[u8]) -> Result<EszipV1, ParseError> {
    let eszip: EszipV1 =
      serde_json::from_slice(data).map_err(ParseError::InvalidV1Json)?;
    if eszip.version != ESZIP_V1_GRAPH_VERSION {
      return Err(ParseError::InvalidV1Version(eszip.version));
    }
    Ok(eszip)
  }

  pub fn into_bytes(self) -> Vec<u8> {
    serde_json::to_vec(&self).unwrap()
  }

  pub fn get_module(&self, specifier: &str) -> Option<Module> {
    let mut specifier = &Url::parse(specifier).ok()?;
    let mut visited = HashSet::new();
    loop {
      visited.insert(specifier);
      let module = self.modules.get(specifier)?;
      match module {
        ModuleInfo::Redirect(redirect) => {
          specifier = redirect;
          if visited.contains(specifier) {
            return None;
          }
        }
        ModuleInfo::Source(source) => {
          let module = Module {
            specifier: specifier.to_string(),
            kind: ModuleKind::JavaScript,
            inner: ModuleInner::V1(Arc::new(
              source
                .transpiled
                .as_ref()
                .unwrap_or(&source.source)
                .as_bytes()
                .to_owned(),
            )),
          };
          return Some(module);
        }
      }
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ModuleInfo {
  Redirect(Url),
  Source(ModuleSource),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleSource {
  pub source: String,
  pub transpiled: Option<String>,
  pub content_type: Option<String>,
  pub deps: Vec<Url>,
}

#[cfg(test)]
mod tests {
  use crate::EszipV1;

  #[test]
  fn file_format_parse() {
    let data = include_bytes!("./testdata/basic.json");
    let eszip = EszipV1::parse(data).unwrap();
    assert_eq!(eszip.version, 1);
    assert_eq!(eszip.modules.len(), 1);
    let module = eszip.get_module("https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js").unwrap();
    assert_eq!(module.specifier, "https://gist.githubusercontent.com/lucacasonato/f3e21405322259ca4ed155722390fda2/raw/e25acb49b681e8e1da5a2a33744b7a36d538712d/hello.js");
    let inner = module.inner;
    let bytes = match inner {
      crate::ModuleInner::V1(bytes) => bytes,
      crate::ModuleInner::V2(_) => unreachable!(),
    };
    assert_eq!(*bytes, b"addEventListener(\"fetch\", (event)=>{\n    event.respondWith(new Response(\"Hello World\", {\n        headers: {\n            \"content-type\": \"text/plain\"\n        }\n    }));\n});\n//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJzb3VyY2VzIjpbIjxodHRwczovL2dpc3QuZ2l0aHVidXNlcmNvbnRlbnQuY29tL2x1Y2FjYXNvbmF0by9mM2UyMTQwNTMyMjI1OWNhNGVkMTU1NzIyMzkwZmRhMi9yYXcvZTI1YWNiNDliNjgxZThlMWRhNWEyYTMzNzQ0YjdhMzZkNTM4NzEyZC9oZWxsby5qcz4iXSwic291cmNlc0NvbnRlbnQiOlsiYWRkRXZlbnRMaXN0ZW5lcihcImZldGNoXCIsIChldmVudCkgPT4ge1xuICBldmVudC5yZXNwb25kV2l0aChuZXcgUmVzcG9uc2UoXCJIZWxsbyBXb3JsZFwiLCB7XG4gICAgaGVhZGVyczogeyBcImNvbnRlbnQtdHlwZVwiOiBcInRleHQvcGxhaW5cIiB9LFxuICB9KSk7XG59KTsiXSwibmFtZXMiOltdLCJtYXBwaW5ncyI6IkFBQUEsZ0JBQUEsRUFBQSxLQUFBLElBQUEsS0FBQTtBQUNBLFNBQUEsQ0FBQSxXQUFBLEtBQUEsUUFBQSxFQUFBLFdBQUE7QUFDQSxlQUFBO2FBQUEsWUFBQSxJQUFBLFVBQUEifQ==");
  }

  #[tokio::test]
  async fn get_transpiled_for_ts() {
    let data = include_bytes!("./testdata/dotland.json");
    let eszip = EszipV1::parse(data).unwrap();
    assert_eq!(eszip.version, 1);

    let module = eszip.get_module("file:///src/worker/handler.ts").unwrap();
    assert_eq!(module.specifier, "file:///src/worker/handler.ts");
    let bytes = module.source().await;
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(!text.contains("import type { ConnInfo }"));
  }
}
