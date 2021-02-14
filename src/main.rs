use anyhow::Error;
use std::fs::File;

mod load_modules;
mod parse_deps;
mod resolve_import;

use load_modules::load_modules;
use url::Url;

fn usage() -> ! {
  eprintln!("Bad subcommand. Do something like this:");
  eprintln!(
        "> estar new kament.tar https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts"
    );
  eprintln!(
        "> estar read kament.tar https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts"
    );
  std::process::exit(1)
}

async fn subcommand_new(
  tar_filename: String,
  root: String,
) -> Result<(), Error> {
  println!("tar file: {}", tar_filename);
  // println!("url: {}", url);

  let root = Url::parse(&root)?;

  let modules = load_modules(root).await?;

  let tar_file = File::create(tar_filename).unwrap();
  let mut ar = tar::Builder::new(tar_file);

  for (url, info) in modules.iter() {
    let source_bytes = info.source.as_bytes();
    let mut header = tar::Header::new_gnu();
    // header.set_path(url.as_str()).unwrap();
    header.set_size(source_bytes.len() as u64);
    header.set_cksum();
    ar.append_data(&mut header, url.as_str(), source_bytes)
      .unwrap();
  }

  ar.finish().unwrap();
  Ok(())
}

#[tokio::main]
async fn main() {
  let mut args = std::env::args();
  match args.nth(1).as_deref() {
    Some("new") => {
      let tar_filename = match args.next() {
        Some(t) => t,
        None => {
          eprintln!("First arg should be tarball");
          usage()
        }
      };
      let url = match args.next() {
        Some(t) => t,
        None => {
          eprintln!("Second arg should be a URL");
          usage()
        }
      };
      subcommand_new(tar_filename, url).await.unwrap()
    }
    Some("read") => {
      todo!()
    }
    _ => {
      usage();
    }
  }
}
