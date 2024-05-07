use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use deno_ast::{EmitOptions, TranspileOptions};
use deno_graph::{
  source::{MemoryLoader, Source},
  BuildOptions, CapturingModuleAnalyzer, GraphKind, ModuleGraph,
  ModuleSpecifier,
};
use eszip::{v2::Checksum, EszipV2};
use futures::io::{AllowStdIo, BufReader};

fn into_bytes_sha256(mut eszip: EszipV2) -> Vec<u8> {
  eszip.set_checksum(Checksum::Sha256);
  eszip.into_bytes()
}

fn into_bytes_crc32(mut eszip: EszipV2) -> Vec<u8> {
  eszip.set_checksum(Checksum::Crc32);
  eszip.into_bytes()
}

async fn parse_sha256(bytes: &[u8]) -> EszipV2 {
  let (eszip, fut) = EszipV2::parse(BufReader::new(AllowStdIo::new(bytes)))
    .await
    .unwrap();
  fut.await.unwrap();
  eszip
}

async fn parse_crc32(bytes: &[u8]) -> EszipV2 {
  let (eszip, fut) = EszipV2::parse(BufReader::new(AllowStdIo::new(bytes)))
    .await
    .unwrap();
  fut.await.unwrap();
  eszip
}

fn bench_into_bytes(c: &mut Criterion) {
  let mut group = c.benchmark_group("into_bytes()");
  group.sample_size(10);
  for mb in (1..200).step_by(20) {
    group.throughput(criterion::Throughput::Bytes((mb as u64) << 20));
    group.bench_with_input(
      BenchmarkId::new("SHA256", format!("{mb}MB")),
      &mb,
      |b, mb| {
        b.iter_batched(
          || {
            let rt = tokio::runtime::Builder::new_current_thread()
              .build()
              .unwrap();
            rt.block_on(build_eszip(*mb))
          },
          into_bytes_sha256,
          criterion::BatchSize::SmallInput,
        )
      },
    );
    group.bench_with_input(
      BenchmarkId::new("CRC32", format!("{mb}MB")),
      &mb,
      |b, mb| {
        b.iter_batched(
          || {
            let rt = tokio::runtime::Builder::new_current_thread()
              .build()
              .unwrap();
            rt.block_on(build_eszip(*mb))
          },
          into_bytes_crc32,
          criterion::BatchSize::SmallInput,
        )
      },
    );
  }
  group.finish();
}

fn bench_parse(c: &mut Criterion) {
  let mut group = c.benchmark_group("parse()");
  group.sample_size(10);
  for mb in (1..200).step_by(20) {
    group.throughput(criterion::Throughput::Bytes((mb as u64) << 20));
    let rt = tokio::runtime::Builder::new_current_thread()
      .build()
      .unwrap();
    let mut eszip = rt.block_on(build_eszip(mb));
    eszip.set_checksum(Checksum::Sha256);
    let bytes = eszip.into_bytes();
    group.bench_with_input(
      BenchmarkId::new("SHA256", format!("{mb}MB")),
      &bytes,
      |b, bytes| b.to_async(&rt).iter(|| parse_sha256(bytes)),
    );
    let mut eszip = rt.block_on(build_eszip(mb));
    eszip.set_checksum(Checksum::Crc32);
    let bytes = eszip.into_bytes();
    group.bench_with_input(
      BenchmarkId::new("CRC32", format!("{mb}MB")),
      &bytes,
      |b, bytes| b.to_async(&rt).iter(|| parse_crc32(bytes)),
    );
  }
  group.finish();
}

criterion_group!(benches, bench_into_bytes, bench_parse);
criterion_main!(benches);

async fn build_eszip(mb: usize) -> EszipV2 {
  let roots = vec![ModuleSpecifier::parse("file:///module1.js").unwrap()];
  let analyzer = CapturingModuleAnalyzer::default();
  let mut graph = ModuleGraph::new(GraphKind::CodeOnly);
  let mut sources = vec![(
    String::from("file:///module1.js"),
    Source::Module {
      specifier: String::from("file:///module1.js"),
      maybe_headers: None,
      content: (2..=mb)
        .map(|x| format!(r#"import "./module{x}.js";"#))
        .chain([build_comment_module(1)])
        .collect::<Vec<String>>()
        .join("\n"),
    },
  )];
  for x in 2..=mb {
    let specifier = format!("file:///module{x}.js");
    sources.push((
      specifier.clone(),
      Source::Module {
        specifier,
        maybe_headers: None,
        content: build_comment_module(1),
      },
    ))
  }
  let loader = MemoryLoader::new(sources, Vec::new());
  graph
    .build(
      roots,
      &loader,
      BuildOptions {
        module_analyzer: &analyzer,
        ..Default::default()
      },
    )
    .await;
  graph.valid().unwrap();
  EszipV2::from_graph(
    graph,
    &analyzer.as_capturing_parser(),
    TranspileOptions::default(),
    EmitOptions::default(),
  )
  .unwrap()
}

fn build_comment_module(mb: usize) -> String {
  format!("// {}", "a".repeat(mb << 20))
}