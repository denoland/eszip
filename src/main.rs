use anyhow::Error;
use eszip::load_reqwest;
use std::fs::File;
use url::Url;

fn usage() -> ! {
  eprintln!("Bad subcommand. Do something like this:");
  eprintln!(
    "> eszip get https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts"
  );
  eprintln!("> eszip list es.zip");
  eprintln!("> eszip read es.zip https://deno.land/x/djwt@v2.1/mod.ts");
  std::process::exit(1)
}

fn subcommand_read(zip_filename: String, url: String) -> Result<(), Error> {
  let zip_file = File::open(zip_filename)?;
  let mut zip = eszip::ZipReader::new(zip_file)?;

  let url = Url::parse(&url)?;
  let source = zip.get_source(&url)?;
  print!("{}", source);
  Ok(())
}

fn subcommand_list(zip_filename: String) -> Result<(), Error> {
  let zip_file = File::open(zip_filename)?;
  let mut zip = eszip::ZipReader::new(zip_file)?;

  for i in 0..zip.len() {
    let url = zip.url_by_index(i)?;
    println!("{}", url);
    // std::io::copy(&mut file, &mut std::io::stdout());
  }
  Ok(())
}

async fn subcommand_get(
  zip_filename: String,
  root: String,
) -> Result<(), Error> {
  let root = Url::parse(&root)?;

  let zip_file = File::create(&zip_filename)?;
  let mut zip = eszip::ZipWriter::new(zip_file);

  use futures::stream::TryStreamExt;
  let mut stream = load_reqwest(root);
  let mut seen = 0;

  let bar = indicatif::ProgressBar::new(0).with_style(
    indicatif::ProgressStyle::default_bar()
      .template("{pos:>3}/{len:3} {wide_msg}"),
  );

  while let Some(info) = stream.try_next().await? {
    seen += 1;
    bar.set_position(seen as u64);
    bar.set_length(stream.total() as u64);
    bar.set_message(info.url.as_str());

    zip.add_module(&info.url, &info.source)?;
  }
  bar.finish();

  println!("Wrote {}", zip_filename);

  zip.finish()?;
  Ok(())
}

#[tokio::main]
async fn main() {
  let mut args = std::env::args();
  match args.nth(1).as_deref() {
    Some("get") => {
      let url = match args.next() {
        Some(t) => t,
        None => {
          eprintln!("Expected a URL argument");
          usage()
        }
      };
      let zip_filename = match args.next() {
        Some(t) => t,
        None => "es.zip".to_string(),
      };
      subcommand_get(zip_filename, url).await.unwrap()
    }
    Some("list") => {
      let zip_filename = match args.next() {
        Some(t) => t,
        None => {
          eprintln!("Expected a zip filename");
          usage()
        }
      };
      subcommand_list(zip_filename).unwrap()
    }
    Some("read") => {
      let zip_filename = match args.next() {
        Some(t) => t,
        None => {
          eprintln!("Expected a zip filename");
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
      subcommand_read(zip_filename, url).unwrap()
    }
    _ => {
      usage();
    }
  }
}
