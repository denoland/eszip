use crate::error::Error;
use crate::resolve_import::resolve_import;
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
use swc_ecmascript::ast::Program;
use swc_ecmascript::dep_graph::analyze_dependencies;
use swc_ecmascript::dep_graph::DependencyKind;
use swc_ecmascript::parser::lexer::Lexer;
use swc_ecmascript::parser::EsConfig;
use swc_ecmascript::parser::JscTarget;
use swc_ecmascript::parser::Parser;
use swc_ecmascript::parser::Syntax;
use swc_ecmascript::parser::TsConfig;
use url::Url;

// Returns (deps, transpiled source code)
pub fn get_deps_and_transpile(
  url: &Url,
  source: &str,
  content_type: &Option<String>,
) -> Result<(Vec<Url>, Option<String>), Error> {
  let comments = SingleThreadedComments::default();
  let source_map = SourceMap::default();
  let source_file = source_map
    .new_source_file(FileName::Custom(url.to_string()), source.to_string());
  let input = StringInput::from(&*source_file);
  let syntax = get_syntax(url, content_type);
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

  // If the file is not jsx, ts, or tsx we do not need to transform it. In that
  // case source == transformed.
  if !syntax.jsx() && !syntax.typescript() {
    return Ok((deps, None));
  }

  use swc_ecmascript::transforms::react;

  let program = Program::Module(module);

  let options = EmitOptions::default();
  let source_map = std::rc::Rc::new(source_map);

  let jsx_pass = react::react(
    source_map.clone(),
    Some(&comments),
    react::Options {
      pragma: options.jsx_factory.clone(),
      pragma_frag: options.jsx_fragment_factory.clone(),
      // this will use `Object.assign()` instead of the `_extends` helper
      // when spreading props.
      use_builtins: true,
      ..Default::default()
    },
  );

  use swc_common::chain;
  use swc_common::Globals;
  use swc_ecmascript::transforms::fixer;
  use swc_ecmascript::transforms::helpers;
  use swc_ecmascript::transforms::pass::Optional;
  use swc_ecmascript::transforms::proposals;
  use swc_ecmascript::transforms::typescript;
  use swc_ecmascript::visit::FoldWith;

  let mut passes = chain!(
    Optional::new(jsx_pass, options.transform_jsx),
    proposals::decorators::decorators(proposals::decorators::Config {
      legacy: true,
      emit_metadata: options.emit_metadata
    }),
    helpers::inject_helpers(),
    typescript::strip(),
    fixer(Some(&comments)),
  );

  let program = swc_common::GLOBALS.set(&Globals::new(), || {
    helpers::HELPERS.set(&helpers::Helpers::new(false), || {
      program.fold_with(&mut passes)
    })
  });

  use swc_ecmascript::codegen::text_writer::JsWriter;
  use swc_ecmascript::codegen::Node;

  let mut src_map_buf = vec![];
  let mut buf = vec![];
  {
    let writer = Box::new(JsWriter::new(
      source_map.clone(),
      "\n",
      &mut buf,
      Some(&mut src_map_buf),
    ));
    let config = swc_ecmascript::codegen::Config { minify: false };
    let mut emitter = swc_ecmascript::codegen::Emitter {
      cfg: config,
      comments: Some(&comments),
      cm: source_map.clone(),
      wr: writer,
    };
    program
      .emit_with(&mut emitter)
      .map_err(|err| Error::Other(Box::new(err)))?;
  }

  let mut src =
    String::from_utf8(buf).map_err(|err| Error::Other(Box::new(err)))?;
  {
    let mut buf = Vec::new();
    source_map
      .build_source_map_from(&mut src_map_buf, None)
      .to_writer(&mut buf)
      .map_err(|err| Error::Other(Box::new(err)))?;

    src.push_str("//# sourceMappingURL=data:application/json;base64,");
    let encoded_map = base64::encode(buf);
    src.push_str(&encoded_map);
  }

  Ok((deps, Some(src)))
}

fn get_syntax(url: &Url, maybe_content_type: &Option<String>) -> Syntax {
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

  let maybe_extension = if let Some(content_type) = maybe_content_type {
    match content_type
      .split(";")
      .next()
      .unwrap()
      .trim()
      .to_lowercase()
      .as_ref()
    {
      "application/typescript"
      | "text/typescript"
      | "video/vnd.dlna.mpeg-tts"
      | "video/mp2t"
      | "application/x-typescript" => Some("ts"),
      "application/javascript"
      | "text/javascript"
      | "application/ecmascript"
      | "text/ecmascript"
      | "application/x-javascript"
      | "application/node" => Some("js"),
      "text/jsx" => Some("jsx"),
      "text/tsx" => Some("tsx"),
      _ => None,
    }
  } else {
    None
  };

  let extension = if maybe_extension.is_some() {
    maybe_extension
  } else {
    let parts: Vec<&str> = url.as_str().split('.').collect();
    parts.last().copied()
  };

  match extension {
    Some("js") => Syntax::Es(get_es_config(false)),
    Some("jsx") => Syntax::Es(get_es_config(true)),
    Some("ts") => Syntax::Typescript(get_ts_config(false, false)),
    Some("tsx") => Syntax::Typescript(get_ts_config(true, false)),
    _ => Syntax::Typescript(get_ts_config(false, false)),
  }
}

pub struct ParseError {
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

/// Options which can be adjusted when transpiling a module.
#[derive(Debug, Clone)]
pub struct EmitOptions {
  /// Indicate if JavaScript is being checked/transformed as well, or if it is
  /// only TypeScript.
  pub check_js: bool,
  /// When emitting a legacy decorator, also emit experimental decorator meta
  /// data.  Defaults to `false`.
  pub emit_metadata: bool,
  /// Should the source map be inlined in the emitted code file, or provided
  /// as a separate file.  Defaults to `true`.
  pub inline_source_map: bool,
  /// When transforming JSX, what value should be used for the JSX factory.
  /// Defaults to `React.createElement`.
  pub jsx_factory: String,
  /// When transforming JSX, what value should be used for the JSX fragment
  /// factory.  Defaults to `React.Fragment`.
  pub jsx_fragment_factory: String,
  /// Should JSX be transformed or preserved.  Defaults to `true`.
  pub transform_jsx: bool,
}

impl Default for EmitOptions {
  fn default() -> Self {
    EmitOptions {
      check_js: false,
      emit_metadata: false,
      inline_source_map: true,
      jsx_factory: "h".into(),
      jsx_fragment_factory: "Fragment".into(),
      transform_jsx: true,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_get_syntax() {
    // Prefer content-type over extension.
    let url = Url::parse("https://deno.land/x/foo@0.1.0/bar.js").unwrap();
    let content_type = Some("text/jsx".to_string());
    let syntax = get_syntax(&url, &content_type);
    assert!(syntax.jsx());
    assert!(!syntax.typescript());

    // Fallback to extension if content-type is unsupported.
    let url = Url::parse("https://deno.land/x/foo@0.1.0/bar.tsx").unwrap();
    let content_type = Some("text/unsupported".to_string());
    let syntax = get_syntax(&url, &content_type);
    assert!(syntax.jsx());
    assert!(syntax.typescript());
  }

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
    let (deps, _transpiled) =
      get_deps_and_transpile(&url, source, &None).unwrap();
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
    let (deps, _transpiled) =
      get_deps_and_transpile(&url, source, &None).unwrap();
    assert_eq!(deps.len(), 0);
  }
}
