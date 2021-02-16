use crate::resolve_import::resolve_import;
use anyhow::Error;
use swc_common::comments::SingleThreadedComments;
use swc_common::input::StringInput;
use swc_common::FileName;
use swc_common::SourceMap;
use swc_ecma_parser::lexer::Lexer;
use swc_ecma_parser::EsConfig;
use swc_ecma_parser::JscTarget;
use swc_ecma_parser::Parser;
use swc_ecma_parser::Syntax;
use swc_ecma_parser::TsConfig;
use swc_ecmascript::dep_graph::analyze_dependencies;
use swc_ecmascript::dep_graph::DependencyKind;
use url::Url;

pub fn parse_deps(url: &Url, source: &str) -> Result<Vec<Url>, Error> {
  let comments = SingleThreadedComments::default();
  let source_map = SourceMap::default();
  let source_file = source_map
    .new_source_file(FileName::Custom(url.to_string()), source.to_string());
  let input = StringInput::from(&*source_file);
  let syntax = get_syntax(url);
  let lexer = Lexer::new(syntax, JscTarget::Es2020, input, Some(&comments));
  let mut parser = Parser::new_from(lexer);
  // TODO(ry) the trait `std::error::Error` is not implemented for
  // `swc_ecmascript::swc_ecma_parser::error::Error`

  /* TODO(ry) Need diagnostics...

    let handler = swc_common::errors::Handler::with_emitter(
      swc_common::errors::ColorConfig::Auto,
      true,
      false,
      None,
    );
  new(
      dst: Box<dyn Write + Send>,
      source_map: Option<Lrc<SourceMapperDyn>>,
      short_message: bool,
      teach: bool
  ) -> EmitterWriter
  */

  for e in parser.take_errors() {
    println!("take err {:?}\n", e);
    // e.into_diagnostic(&handler).emit();
  }

  let module = parser
    .parse_module()
    .map_err(|e| {
      println!("fatal {:?}\n", e);
      e
    })
    .unwrap();
  let mut deps = Vec::new();
  for import in analyze_dependencies(&module, &source_map, &comments) {
    if import.kind == DependencyKind::Import {
      let specifier = import.specifier.to_string();
      deps.push(resolve_import(&specifier, url.as_str())?);
    }
  }
  Ok(deps)
}

fn get_syntax(url: &Url) -> Syntax {
  fn get_es_config(jsx: bool) -> EsConfig {
    EsConfig {
      class_private_methods: true,
      class_private_props: true,
      class_props: true,
      dynamic_import: true,
      export_default_from: true,
      export_namespace_from: true,
      import_meta: true,
      jsx,
      nullish_coalescing: true,
      num_sep: true,
      optional_chaining: true,
      top_level_await: true,
      ..EsConfig::default()
    }
  }

  fn get_ts_config(tsx: bool, dts: bool) -> TsConfig {
    TsConfig {
      decorators: true,
      dts,
      dynamic_import: true,
      tsx,
      ..TsConfig::default()
    }
  }

  let parts: Vec<&str> = url.as_str().split('.').collect();
  match parts.last().map(|p| *p) {
    Some("js") => Syntax::Es(get_es_config(false)),
    Some("jsx") => Syntax::Es(get_es_config(true)),
    Some("ts") => Syntax::Typescript(get_ts_config(false, false)),
    Some("tsx") => Syntax::Typescript(get_ts_config(true, false)),
    _ => Syntax::Es(get_es_config(false)),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn jsx() {
    let url = Url::parse(
      "https://deno.land/x/dext@0.10.3/example/pages/dynamic/%5Bname%5D.tsx",
    )
    .unwrap();
    let source = r#"
      import { Fragment, h } from "../../deps.ts";
      import type { PageProps } from "../../deps.ts";
      function UserPage(props: PageProps) {
        const name = props.route?.name ?? "";

        return (
          <>
            <h1>This is the page for {name}</h1>
            <p> <a href="/">Go home</a> </p>
          </>
        );
      }

      export default UserPage;
    "#;
    let deps = parse_deps(&url, source).unwrap();
    assert_eq!(deps.len(), 2);
  }

  #[test]
  #[ignore]
  fn complex_types() {
    let url = Url::parse("https://deno.land/x/oak@v6.4.2/router.ts").unwrap();
    let source = r#"
      delete<P extends RouteParams = RP, S extends State = RS>(
        name: string,
        path: string,
        ...middleware: RouterMiddleware<P, S>[]
      ): Router<P extends RP ? P : (P & RP), S extends RS ? S : (S & RS)>;
    "#;
    let deps = parse_deps(&url, source).unwrap();
    assert_eq!(deps.len(), 0);
  }
}
