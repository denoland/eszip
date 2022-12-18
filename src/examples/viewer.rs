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
      let module = eszip.get_module(&specifier).unwrap();
      if module.specifier == specifier {
        println!("Specifier: {}", specifier);
        println!("Kind: {:?}", module.kind);

        let source = module.source().await;
        let source = std::str::from_utf8(&source).unwrap();
        println!("---");
        println!("{}", source);

        let source_map = module.source_map().await;
        if let Some(source_map) = source_map {
          let source_map = std::str::from_utf8(&source_map).unwrap();
          println!("---");
          println!("{}", source_map);
        }

        println!("============");
      }
    }

    Ok(())
  };

  tokio::try_join!(loader, fut).unwrap();
}
