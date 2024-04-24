// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

use std::path::Path;

use futures::io::AllowStdIo;
use futures::io::BufReader;
use url::Url;

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let args = std::env::args().collect::<Vec<_>>();
  let path = args.get(1).unwrap();

  let file = std::fs::File::open(path).unwrap();
  let bufreader = BufReader::new(AllowStdIo::new(file));
  let (eszip, loader) = eszip::EszipV2::parse(bufreader).await.unwrap();

  let fut = async move {
    for (_, module) in eszip {
      if module.specifier.starts_with("file://") {
        let path = Url::parse(&module.specifier)
          .unwrap()
          .to_file_path()
          .unwrap();

        let cwd = std::env::current_dir().unwrap();
        let absolute_path = &format!(
          "{cwd}{path}",
          cwd = cwd.to_str().unwrap(),
          path = path.to_str().unwrap()
        );
        println!("{:?}", absolute_path);
        std::fs::create_dir_all(Path::new(&absolute_path).parent().unwrap())
          .unwrap();
        std::fs::write(absolute_path, module.source().await.unwrap()).unwrap();
      }
      // if module.specifier == specifier {
      //   println!("Specifier: {specifier}",);
      //   println!("Kind: {kind:?}", kind = module.kind);

      //   let source = module.source().await.expect("source already taken");
      //   let source = std::str::from_utf8(&source).unwrap();
      //   println!("---");
      //   println!("{source}");

      //   let source_map = module.source_map().await;
      //   if let Some(source_map) = source_map {
      //     let source_map = std::str::from_utf8(&source_map).unwrap();
      //     println!("---");
      //     println!("{source_map}");
      //   }

      //   println!("============");
      // }
    }

    Ok(())
  };

  tokio::try_join!(loader, fut).unwrap();
}
