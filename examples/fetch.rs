// Downloads module graph and serializes it to JSON on stdout.
// cargo run --example fetch https://deno.land/x/oak/router.ts

use anyhow::Error;
use eszip::load_reqwest;
use eszip::ModuleInfo;
use futures::stream::TryStreamExt;
use std::collections::HashMap;
use url::Url;

type ModuleGraph = HashMap<Url, ModuleInfo>;

async fn fetch(root: Url) -> Result<(), Error> {
  //let zip_file = File::create(&zip_filename)?;
  //let mut zip = eszip::ZipWriter::new(zip_file);

  let mut stream = load_reqwest(root, reqwest::ClientBuilder::new());
  let mut seen = 0;

  let mut graph = ModuleGraph::new();

  let bar = indicatif::ProgressBar::new(0).with_style(
    indicatif::ProgressStyle::default_bar()
      .template("{pos:>3}/{len:3} {wide_msg}"),
  );

  while let Some((url, info)) = stream.try_next().await? {
    seen += 1;
    bar.set_position(seen as u64);
    bar.set_length(stream.total() as u64);
    bar.set_message(url.as_str());

    graph.insert(url, info);
  }
  bar.finish();

  serde_json::to_writer_pretty(std::io::stdout(), &graph).unwrap();
  println!();

  Ok(())
}

#[tokio::main]
async fn main() {
  let root = std::env::args().nth(1).unwrap();
  let root = Url::parse(&root).unwrap();
  fetch(root).await.unwrap()
}
