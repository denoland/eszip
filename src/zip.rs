//! Wrappers around zip utilities
use anyhow::Error;
use std::collections::HashMap;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use url::Url;
pub use zip::result::ZipError;

pub struct ZipReader<R: Read + Seek>(zip::ZipArchive<R>);

impl<R: Read + Seek> ZipReader<R> {
  pub fn new(reader: R) -> Result<ZipReader<R>, ZipError> {
    let zip = zip::ZipArchive::new(reader)?;

    let comment = std::str::from_utf8(zip.comment()).unwrap();
    if comment.starts_with("eszip/") {
      Ok(Self(zip))
    } else {
      Err(ZipError::UnsupportedArchive(
        "Bad eszip file, expected comment to start with 'eszip'",
      ))
    }
  }

  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  pub fn into_hashmap(mut self) -> Result<HashMap<Url, String>, ZipError> {
    let mut hm = HashMap::new();
    for i in 0..self.len() {
      let url = self.url_by_index(i)?;
      let source = self.get_source(&url)?;
      hm.insert(url, source);
    }
    Ok(hm)
  }

  pub fn url_by_index(&mut self, idx: usize) -> Result<Url, ZipError> {
    let file = self.0.by_index(idx)?;
    let url = filename_to_url(file.name().to_string())
      .map_err(|_| ZipError::InvalidArchive("could not base64 decode url"))?;
    Ok(url)
  }

  pub fn get_source(&mut self, url: &Url) -> Result<String, ZipError> {
    let filename = url_to_filename(url);
    let mut file = self.0.by_name(&filename)?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;
    Ok(buffer)
  }
}

pub struct ZipWriter<W: Write + Seek>(zip::write::ZipWriter<W>);

impl<W: Write + Seek> ZipWriter<W> {
  pub fn new(writer: W) -> ZipWriter<W> {
    let mut zip = zip::ZipWriter::new(writer);
    zip.set_comment(concat!("eszip/", env!("CARGO_PKG_VERSION")));
    Self(zip)
  }

  pub fn add_module(
    &mut self,
    url: &Url,
    source: &str,
  ) -> Result<(), ZipError> {
    let filename = url_to_filename(url);
    self
      .0
      .start_file(filename, zip::write::FileOptions::default())?;
    self.0.write_all(source.as_bytes())?;
    Ok(())
  }

  pub fn finish(&mut self) -> Result<W, ZipError> {
    self.0.finish()
  }
}

fn url_to_filename(url: &Url) -> String {
  base64::encode(url.as_str().as_bytes())
}

fn filename_to_url(filename: String) -> Result<Url, Error> {
  let d = base64::decode(filename)?;
  let s = std::str::from_utf8(&d)?;
  let u = Url::parse(s)?;
  Ok(u)
}

#[test]
fn url_to_filename_and_back() {
  let url = Url::parse("https://raw.githubusercontent.com/denoland/deno/5873adeb5e6ec2113eeb5adc964b7ce129d4905d/cli/tests/subdir/print_hello.ts").unwrap();
  let filename = url_to_filename(&url);
  let url_ = filename_to_url(filename).unwrap();
  assert_eq!(url, url_);
}

#[test]
fn there_and_back_again() {
  let mut w = ZipWriter::new(std::io::Cursor::new(Vec::new()));
  let foo_url = Url::parse("file:///foo.ts").unwrap();
  let foo_source = "let a = 1";
  w.add_module(&foo_url, &foo_source).unwrap();
  let bar_url = Url::parse("file:///bar.js").unwrap();
  let bar_source = "let b = 'hi';";
  w.add_module(&bar_url, &bar_source).unwrap();
  let cursor = w.finish().unwrap();

  let r = ZipReader::new(cursor).unwrap();
  let hm = r.into_hashmap().unwrap();
  assert_eq!(hm.len(), 2);
  assert_eq!(hm.get(&foo_url).unwrap(), foo_source);
  assert_eq!(hm.get(&bar_url).unwrap(), bar_source);
}
