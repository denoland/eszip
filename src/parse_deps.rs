use crate::resolve_import::resolve_import;
use anyhow::Error;
use std::sync::Arc;
use std::sync::Mutex;
use swc_common::comments::SingleThreadedComments;
use swc_common::errors::Diagnostic;
use swc_common::errors::DiagnosticBuilder;
use swc_common::errors::Emitter;
use swc_common::errors::Handler;
use swc_common::errors::HandlerFlags;
use swc_common::input::StringInput;
use swc_common::FileName;
use swc_common::SourceMap;
use swc_ecmascript::dep_graph::analyze_dependencies;
use swc_ecmascript::dep_graph::DependencyKind;
use swc_ecmascript::parser::lexer::Lexer;
use swc_ecmascript::parser::EsConfig;
use swc_ecmascript::parser::JscTarget;
use swc_ecmascript::parser::Parser;
use swc_ecmascript::parser::Syntax;
use swc_ecmascript::parser::TsConfig;
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

  let module = parser
    .parse_module()
    .map_err(|e| ParseError::new(e, &source_map))?;
  let mut deps = Vec::new();
  for import in analyze_dependencies(&module, &source_map, &comments) {
    if import.kind == DependencyKind::Import
      || import.kind == DependencyKind::Export
    {
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
  match parts.last().copied() {
    Some("js") => Syntax::Es(get_es_config(false)),
    Some("jsx") => Syntax::Es(get_es_config(true)),
    Some("ts") => Syntax::Typescript(get_ts_config(false, false)),
    Some("tsx") => Syntax::Typescript(get_ts_config(true, false)),
    _ => Syntax::Typescript(get_ts_config(false, false)),
  }
}

struct ParseError {
  lines: Vec<String>,
}

impl ParseError {
  fn new(
    err: swc_ecmascript::parser::error::Error,
    source_map: &SourceMap,
  ) -> Self {
    let error_buffer = ErrorBuffer::default();
    let handler = Handler::with_emitter_and_flags(
      Box::new(error_buffer.clone()),
      HandlerFlags {
        can_emit_warnings: true,
        dont_buffer_diagnostics: true,
        ..HandlerFlags::default()
      },
    );
    let mut diagnostic = err.into_diagnostic(&handler);
    diagnostic.emit();

    let v = error_buffer.0.lock().unwrap();
    let lines = v
      .iter()
      .map(|d| {
        if let Some(span) = d.span.primary_span() {
          let loc = source_map.lookup_char_pos(span.lo);
          let file_name = match &loc.file.name {
            FileName::Custom(n) => n,
            _ => unreachable!(),
          };
          format!(
            "{} at {}:{}:{}",
            d.message(),
            file_name,
            loc.line,
            loc.col_display
          )
        } else {
          d.message()
        }
      })
      .collect::<Vec<_>>();
    Self { lines }
  }
}

impl std::error::Error for ParseError {}

impl std::fmt::Display for ParseError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for line in &self.lines {
      writeln!(f, "{}", line)?;
    }
    Ok(())
  }
}

impl std::fmt::Debug for ParseError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    std::fmt::Display::fmt(self, f)
  }
}

/// A buffer for collecting errors from the AST parser.
#[derive(Debug, Clone, Default)]
pub struct ErrorBuffer(Arc<Mutex<Vec<Diagnostic>>>);

impl Emitter for ErrorBuffer {
  fn emit(&mut self, db: &DiagnosticBuilder) {
    self.0.lock().unwrap().push((**db).clone());
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
    assert_eq!(deps.len(), 1);
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
