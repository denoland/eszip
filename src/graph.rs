use crate::ModuleInfo;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Deref;
use std::ops::DerefMut;
use url::Url;

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleGraph {
  version: String,
  modules: HashMap<Url, ModuleInfo>,
}

impl ModuleGraph {
  pub fn is_complete(&self) -> bool {
    let mut references = HashSet::<Url>::new();
    for module_info in self.modules.values() {
      match module_info {
        ModuleInfo::Redirect(u) => {
          references.insert(u.clone());
        }
        ModuleInfo::Source { deps, .. } => {
          for d in deps {
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
      version: format!("eszip/{}", env!("CARGO_PKG_VERSION")),
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
}
