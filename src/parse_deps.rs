use crate::resolve_import::resolve_import;
use anyhow::Error;
use swc_common::comments::SingleThreadedComments;
use swc_common::input::StringInput;
use swc_common::FileName;
use swc_common::SourceMap;
use swc_ecmascript::dep_graph::analyze_dependencies;
use swc_ecmascript::parser::lexer::Lexer;
use swc_ecmascript::parser::JscTarget;
use swc_ecmascript::parser::Syntax;
use swc_ecmascript::parser::TsConfig;
use url::Url;

pub fn parse_deps(url: &Url, source: &str) -> Result<Vec<Url>, Error> {
  let comments = SingleThreadedComments::default();
  let source_map = SourceMap::default();
  let source_file = source_map
    .new_source_file(FileName::Custom(url.to_string()), source.to_string());
  let input = StringInput::from(&*source_file);
  let syntax = Syntax::Typescript(TsConfig {
    tsx: true,
    ..TsConfig::default()
  });
  let lexer = Lexer::new(syntax, JscTarget::Es2020, input, Some(&comments));
  let mut parser = swc_ecmascript::parser::Parser::new_from(lexer);
  // TODO(ry) the trait `std::error::Error` is not implemented for
  // `swc_ecmascript::swc_ecma_parser::error::Error`
  let module = parser.parse_module().unwrap();
  let mut deps = Vec::new();
  for import in analyze_dependencies(&module, &source_map, &comments) {
    let specifier = import.specifier.to_string();
    deps.push(resolve_import(&specifier, url.as_str())?);
  }
  Ok(deps)
}
