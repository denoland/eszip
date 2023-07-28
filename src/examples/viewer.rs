use futures::io::AllowStdIo;
use futures::io::BufReader;

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let args = std::env::args().collect::<Vec<_>>();
  let path = args.get(1).unwrap();

  let file = std::fs::File::open(path).unwrap();
  let bufreader = BufReader::new(AllowStdIo::new(file));
  let (eszip, loader) = eszip::EszipV2::parse(bufreader).await.unwrap();

  let fut = async move {
    let specifiers = eszip.specifiers();
    for specifier in specifiers {
      let module =
        if specifier.ends_with(".json") || specifier.ends_with(".jsonc") {
          eszip
            .get_import_map(&specifier)
            .unwrap_or_else(|| panic!("specifier not found {specifier}"))
        } else {
          eszip
            .get_module(&specifier)
            .unwrap_or_else(|| panic!("specifier not found {specifier}"))
        };
      if module.specifier == specifier {
        println!("Specifier: {specifier}",);
        println!("Kind: {kind:?}", kind = module.kind);

        let source = module.source().await.expect("source already taken");
        let source = std::str::from_utf8(&source).unwrap();
        println!("---");
        println!("{source}");

        let source_map = module.source_map().await;
        if let Some(source_map) = source_map {
          let source_map = std::str::from_utf8(&source_map).unwrap();
          println!("---");
          println!("{source_map}");
        }

        println!("============");
      }
    }

    Ok(())
  };

  tokio::try_join!(loader, fut).unwrap();
}
