// Downloads module graph and serializes it to JSON on stdout.
// cargo run --example fetch https://deno.land/x/oak/router.ts

use eszip::load_reqwest;
use eszip::none_middleware;
use eszip::Error;
use eszip::ModuleGraph;
use futures::stream::TryStreamExt;
use url::Url;

async fn fetch(root: Url) -> Result<(), Error> {
  let mut stream =
    load_reqwest(root, reqwest::ClientBuilder::new(), none_middleware);
  let mut seen = 0;

  let mut graph = ModuleGraph::default();

  let bar = indicatif::ProgressBar::new(0).with_style(
    indicatif::ProgressStyle::default_bar()
      .template("{pos:>3}/{len:3} {wide_msg}"),
  );

  while let Some((url, info)) = stream.try_next().await? {
    seen += 1;
    bar.set_position(seen as u64);
    bar.set_length(stream.total() as u64);
    bar.set_message(url.to_string());

    graph.insert(url, info);
  }
  bar.finish();

  assert!(graph.is_complete());

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
