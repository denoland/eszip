use crate::ModuleInfo;
use crate::ModuleSource;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Deref;
use std::ops::DerefMut;
use url::Url;

pub const GRAPH_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleGraph {
  version: u32,
  modules: HashMap<Url, ModuleInfo>,
}

impl ModuleGraph {
  /// Follow redirects until arriving at the final url and module info.
  pub fn get_redirect(&self, url: &Url) -> Option<(Url, &ModuleSource)> {
    let mut seen = HashSet::<Url>::new();
    let mut current = url.clone();
    let max = self.modules.len();
    let mut i = 0;
    loop {
      if !seen.insert(current.clone()) {
        return None; // infinite loop detected
      }
      match self.modules.get(&current) {
        None => {
          return None;
        }
        Some(ModuleInfo::Redirect(to)) => {
          current = to.clone();
        }
        Some(ModuleInfo::Source(module_source)) => {
          return Some((current, module_source));
        }
      }
      i += 1;
      assert!(i <= max);
    }
  }

  pub fn is_complete(&self) -> bool {
    let mut references = HashSet::<Url>::new();
    for module_info in self.modules.values() {
      match module_info {
        ModuleInfo::Redirect(u) => {
          references.insert(u.clone());
        }
        ModuleInfo::Source(module_source) => {
          for d in &module_source.deps {
            references.insert(d.clone());
          }
        }
      }
    }
    for reference in references {
      if !self.modules.contains_key(&reference) {
        return false;
      }
    }
    true
  }
}

impl Default for ModuleGraph {
  fn default() -> Self {
    ModuleGraph {
      version: GRAPH_VERSION,
      modules: HashMap::new(),
    }
  }
}

impl Deref for ModuleGraph {
  type Target = HashMap<Url, ModuleInfo>;

  fn deref(&self) -> &Self::Target {
    &self.modules
  }
}

impl DerefMut for ModuleGraph {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.modules
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn is_complete() {
    let mut g = ModuleGraph::default();
    assert!(g.is_complete());
    let u = Url::parse("http://deno.land/std/http/foo.ts").unwrap();
    let u2 = Url::parse("http://deno.land/std/http/bar.ts").unwrap();
    g.insert(u, ModuleInfo::Redirect(u2));
    assert!(!g.is_complete());
  }

  #[test]
  fn get_redirect() {
    let mut g = ModuleGraph::default();
    let u1 = Url::parse("http://deno.land/u1.js").unwrap();
    let u2 = Url::parse("http://deno.land/u2.js").unwrap();
    let u3 = Url::parse("http://deno.land/u3.js").unwrap();

    g.insert(u1.clone(), ModuleInfo::Redirect(u2.clone()));
    g.insert(
      u2.clone(),
      ModuleInfo::Source(ModuleSource {
        source: "source".to_string(),
        transpiled: Some("transpiled".to_string()),
        deps: Vec::new(),
        content_type: None,
      }),
    );
    let (final_url, module_source) = g.get_redirect(&u1).unwrap();
    assert_eq!(final_url, u2);
    assert_eq!(module_source.source, "source");
    assert_eq!(module_source.get_code(), "transpiled");

    let (final_url, module_source) = g.get_redirect(&u2).unwrap();
    assert_eq!(final_url, u2);
    assert_eq!(module_source.source, "source");
    assert_eq!(module_source.get_code(), "transpiled");

    assert!(g.get_redirect(&u3).is_none());
  }
}
