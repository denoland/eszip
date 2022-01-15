use bytes::Buf;
use bytes::BytesMut;
use std::convert::TryFrom;
use std::ops::Range;
use tokio_util::codec::Decoder;

#[derive(Debug, PartialEq, Clone)]
pub struct DataPointer(usize, usize);

impl DataPointer {
  pub fn offset(&self) -> usize {
    self.0
  }

  pub fn size(&self) -> usize {
    self.1
  }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Clone)]
pub enum ModuleKind {
  TypeScript,
  JavaScript,
  Jsx,
  Tsx,
}

impl std::convert::TryFrom<u8> for ModuleKind {
  type Error = ();

  fn try_from(v: u8) -> Result<Self, Self::Error> {
    match v {
      x if x == ModuleKind::TypeScript as u8 => Ok(ModuleKind::TypeScript),
      x if x == ModuleKind::JavaScript as u8 => Ok(ModuleKind::JavaScript),
      x if x == ModuleKind::Jsx as u8 => Ok(ModuleKind::Jsx),
      x if x == ModuleKind::Tsx as u8 => Ok(ModuleKind::Tsx),
      _ => Err(()),
    }
  }
}

#[derive(Debug, PartialEq, Clone)]
pub enum HeaderFrame {
  // specifier => (offset, length) pointer to data section
  // TODO(@littledivy): move this to a struct
  Module(String, DataPointer, DataPointer, ModuleKind),
  // specifier => specifier
  Redirect(String, String),
}

#[derive(Debug, PartialEq)]
#[repr(u8)]
pub enum HeaderFrameKind {
  Module = 0,
  Redirect = 1,
}

impl std::convert::TryFrom<u8> for HeaderFrameKind {
  type Error = ();

  fn try_from(v: u8) -> Result<Self, Self::Error> {
    match v {
      x if x == HeaderFrameKind::Module as u8 => Ok(HeaderFrameKind::Module),
      x if x == HeaderFrameKind::Redirect as u8 => {
        Ok(HeaderFrameKind::Redirect)
      }
      _ => Err(()),
    }
  }
}

const ESZIP_V2: &[u8] = b"ESZIP_V2";

#[derive(Default)]
pub struct Header {
  header_size: usize,
  // Used to track the current position in the header
  frame_offset: usize,
  checksum: [u8; 32],
}

impl Header {
  pub fn reset(&mut self) {
    *self = Default::default();
  }

  pub fn checksum(&self) -> &[u8; 32] {
    &self.checksum
  }

  /// Size of the file header (in bytes) from the beginning.
  /// Useful for calculating the offset of the data section.
  pub fn header_size(&self) -> usize {
    // magic + size marker (n) + n + checksum
    8 + 4 + self.header_size + 32
  }
}

// Eszip:
// | Magic (8) | Header size (4) | Header (n) | Header hash (32) | Sources size (4) | Sources (n) | SourceMaps size (4) | SourceMaps (n) |
//
// Header:
// ( | Specifier size (4) | Specifier (n) | Entry type (1) | Entry (n) | )*
//
// Entry (redirect):
// | Specifier size (4) | Specifier (n) |
//
// Entry (module):
// Source offset (4) | Source size (4) | SourceMap offset (4) | SourceMap size (4) | Module type (1) |
//
// Sources:
// ( | Source (n) | Hash (32) | )*
//
// SourceMaps:
// ( | SourceMap (n) | Hash (32) | )*
//
impl Decoder for Header {
  type Item = HeaderFrame;
  type Error = std::io::Error;

  fn decode(
    &mut self,
    buf: &mut BytesMut,
  ) -> Result<Option<Self::Item>, Self::Error> {
    if self.header_size == 0 {
      // Enough to contain magic and header size
      if buf.len() < 8 + 4 {
        return Ok(None);
      }

      let magic = buf.split_to(8);
      if magic != ESZIP_V2 {
        return Err(std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          "Invalid magic",
        ));
      }
      self.header_size = buf.get_u32() as usize;
    }

    if self.frame_offset >= self.header_size {
      if self.frame_offset > self.header_size + 32 {
        // We're done.
        return Ok(None);
      }

      // We've already read the header, but we're left with the
      // checksum.
      if buf.len() < 32 {
        return Ok(None);
      }

      let mut checksum = [0; 32];
      checksum.copy_from_slice(&buf.split_to(32));
      self.checksum = checksum;
      self.frame_offset += 32;

      return Ok(None);
    }

    let initial_len = buf.len();

    // Not enough data
    if buf.len() < 4 {
      return Ok(None);
    }

    // Specifier length marker
    let mut specifier_size = [0; 4];
    specifier_size.copy_from_slice(&buf[..4]);
    let specifier_size = u32::from_be_bytes(specifier_size) as usize;

    // Not enough data to contain specifier and entry type (1)
    if buf.len() < 4 + specifier_size + 1 {
      // Reserve space
      buf.reserve(4 + specifier_size + 1 - buf.len());
      return Ok(None);
    }

    // Specifier
    let specifier = String::from_utf8(buf[4..4 + specifier_size].to_vec())
      .expect("Invalid UTF-8");

    // Entry type
    let entry_type = buf[4 + specifier_size] as u8;
    let entry_kind =
      HeaderFrameKind::try_from(entry_type).expect("Invalid entry type");

    let offset = 4 + specifier_size + 1;
    let frame = match entry_kind {
      HeaderFrameKind::Redirect => {
        if buf.len() < offset + 4 {
          // Reserve space
          buf.reserve(offset + 4 - buf.len());
          return Ok(None);
        }

        // Specifer length marker
        let mut source_size = [0; 4];
        source_size.copy_from_slice(&buf[offset..offset + 4]);
        let source_size = u32::from_be_bytes(source_size) as usize;

        // Not enough data to contain source
        if buf.len() < offset + 4 + source_size {
          // Reserve space
          buf.reserve(offset + 4 + source_size - buf.len());
          return Ok(None);
        }

        // Specifier
        let source =
          String::from_utf8(buf[offset + 4..offset + 4 + source_size].to_vec())
            .expect("Invalid UTF-8");

        buf.advance(offset + 4 + source_size);

        HeaderFrame::Redirect(specifier, source)
      }
      HeaderFrameKind::Module => {
        const FRAME_SIZE: usize = 4 * 4 + 1;

        // Whole frame exists
        if buf.len() < offset + FRAME_SIZE {
          // Reserve space
          buf.reserve(offset + FRAME_SIZE - buf.len());
          return Ok(None);
        }

        // Advance cursor, at point we are sure that we have enough data
        // to read the whole frame.
        buf.advance(offset);

        let source_ptr =
          DataPointer(buf.get_u32() as usize, buf.get_u32() as usize);
        let source_map_ptr =
          DataPointer(buf.get_u32() as usize, buf.get_u32() as usize);
        let module_type =
          ModuleKind::try_from(buf.get_u8()).expect("Invalid module type");

        HeaderFrame::Module(specifier, source_ptr, source_map_ptr, module_type)
      }
    };

    self.frame_offset += initial_len - buf.remaining();
    Ok(Some(frame))
  }
}

impl HeaderFrame {
  pub fn kind(&self) -> HeaderFrameKind {
    match self {
      HeaderFrame::Module(..) => HeaderFrameKind::Module,
      HeaderFrame::Redirect(..) => HeaderFrameKind::Redirect,
    }
  }

  pub fn source_range(&self) -> Option<Range<usize>> {
    match self {
      HeaderFrame::Module(_, source_ptr, _, _) => {
        let DataPointer(start, size) = *source_ptr;
        Some(start..start + size)
      }
      HeaderFrame::Redirect(..) => None,
    }
  }

  pub fn source_map_range(&self) -> Option<Range<usize>> {
    match self {
      HeaderFrame::Module(_, _, source_map_ptr, _) => {
        let DataPointer(start, size) = *source_map_ptr;
        Some(start..start + size)
      }
      HeaderFrame::Redirect(..) => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bytes::BufMut;
  use sha2::Digest;

  fn encode_redirect(specifier: &[u8], redirect: &[u8]) -> BytesMut {
    let mut buf = BytesMut::new();

    buf.put_u32(specifier.len() as u32);
    buf.put(specifier);
    buf.put_u8(HeaderFrameKind::Redirect as u8);

    buf.put_u32(redirect.len() as u32);
    buf.put(redirect);
    buf
  }

  fn encode_module(
    specifier: &[u8],
    source: &[u8],
    source_map: &[u8],
    module_type: ModuleKind,
    // Supply offset for the data section
    maybe_offset: Option<u32>,
    maybe_source_map_offset: Option<u32>,
  ) -> (BytesMut, BytesMut, BytesMut) {
    let mut buf = BytesMut::new();

    buf.put_u32(specifier.len() as u32);
    buf.put(specifier);
    buf.put_u8(HeaderFrameKind::Module as u8);

    let offset = maybe_offset.unwrap_or(0);
    buf.put_u32(offset);
    buf.put_u32(source.len() as u32);
    let source_map_offset = maybe_source_map_offset.unwrap_or(0);
    buf.put_u32(source_map_offset);
    buf.put_u32(source_map.len() as u32);

    buf.put_u8(module_type as u8);

    let mut sources = BytesMut::new();
    sources.put(source);
    let mut hasher = sha2::Sha256::new();
    hasher.update(source);
    let checksum = hasher.finalize();
    sources.put(checksum.as_slice());

    let mut source_maps = BytesMut::new();
    source_maps.put(source_map);
    let mut hasher = sha2::Sha256::new();
    hasher.update(source_map);
    let checksum = hasher.finalize();
    source_maps.put(checksum.as_slice());

    (buf, sources, source_maps)
  }

  fn wrap_header(header: &[BytesMut]) -> (BytesMut, Vec<u8>) {
    let mut buf = BytesMut::new();
    let headers = header.concat();
    buf.put(ESZIP_V2);

    buf.put_u32(headers.len() as u32);
    buf.put(headers.as_ref());

    let mut hasher = sha2::Sha256::new();
    hasher.update(headers);
    let checksum = hasher.finalize();

    buf.put(checksum.as_ref());
    (buf, checksum.to_vec())
  }

  #[test]
  fn encode() {
    let main_source =
      b"import { add } from 'file://add_redirect.js'; add(1, 2);";
    let (module, data, maps) = encode_module(
      b"file://main.js",
      main_source.as_ref(),
      b"".as_ref(),
      ModuleKind::JavaScript,
      None,
      None,
    );
    let (module2, data2, maps2) = encode_module(
      b"file://add.js",
      b"export function add(a, b) { a + b }".as_ref(),
      b"".as_ref(),
      ModuleKind::JavaScript,
      Some(data.len() as u32),
      Some(maps.len() as u32),
    );

    let (mut buf, _) = wrap_header(&[
      encode_redirect(b"file://add_redirect.js", b"file://add.js"),
      module,
      module2,
    ]);
    buf.put(data.as_ref());
    buf.put(data2.as_ref());
    buf.put(maps.as_ref());
    buf.put(maps2.as_ref());

    use std::fs::File;
    use std::io::Write;
    let mut f = File::create("loader.eszip").unwrap();
    f.write_all(buf.as_ref()).unwrap();
  }

  #[test]
  fn test_decode_header() {
    let mut codec = Header::default();

    let mut buf = BytesMut::new();
    assert_eq!(codec.decode(&mut buf).unwrap(), None);

    let (mut buf, _) = wrap_header(&[]);
    assert_eq!(codec.decode(&mut buf).unwrap(), None);

    codec.reset();
    let (mut buf, _) = wrap_header(&[encode_redirect(
      b"https://example.com/foo.js",
      b"https://example.com/bar.js",
    )]);

    assert_eq!(
      codec.decode(&mut buf).unwrap(),
      Some(HeaderFrame::Redirect(
        "https://example.com/foo.js".to_string(),
        "https://example.com/bar.js".to_string()
      ))
    );

    codec.reset();
    let redirect = encode_redirect(
      b"https://example.com/foo.js",
      b"https://example.com/bar.js",
    );
    let (mut buf, checksum) = wrap_header(&[redirect]);
    buf.put(b"ignored".as_ref());
    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(frame.kind(), HeaderFrameKind::Redirect);
    assert_eq!(
      frame,
      HeaderFrame::Redirect(
        "https://example.com/foo.js".to_string(),
        "https://example.com/bar.js".to_string()
      )
    );
    assert_eq!(buf.remaining(), 32 + 7);
    assert_eq!(codec.decode(&mut buf).unwrap(), None);
    assert_eq!(codec.checksum().to_vec(), checksum);
    assert_eq!(buf.remaining(), 7);
    assert_eq!(buf, b"ignored".as_ref());

    codec.reset();
    let (module, _, _) = encode_module(
      b"https://example.com/foo.js",
      b"source".as_ref(),
      b"source_map".as_ref(),
      ModuleKind::JavaScript,
      None,
      None,
    );

    let (mut buf, _) = wrap_header(&[module]);
    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(frame.kind(), HeaderFrameKind::Module);
    assert_eq!(
      frame,
      HeaderFrame::Module(
        "https://example.com/foo.js".to_string(),
        DataPointer(0, 6),
        DataPointer(0, 10),
        ModuleKind::JavaScript
      )
    );
  }

  #[test]
  fn test_decode_mixed() {
    let mut codec = Header::default();
    let (module, data, maps) = encode_module(
      b"https://example.com/foo.js",
      b"source".as_ref(),
      b"source_map".as_ref(),
      ModuleKind::JavaScript,
      None,
      None,
    );
    let (module2, data2, maps2) = encode_module(
      b"https://example.com/bar.js",
      b"source2".as_ref(),
      b"source_map2".as_ref(),
      ModuleKind::JavaScript,
      Some(6),
      Some(10),
    );

    let (mut buf, _) = wrap_header(&[
      encode_redirect(
        b"https://example.com/foo.js",
        b"https://example.com/bar.js",
      ),
      module,
      encode_redirect(
        b"https://example.com/baz.js",
        b"https://example.com/qux.js",
      ),
      module2,
    ]);
    buf.put(data.as_ref());
    buf.put(data2.as_ref());
    buf.put(maps.as_ref());
    buf.put(maps2.as_ref());

    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(frame.kind(), HeaderFrameKind::Redirect);
    assert_eq!(
      frame,
      HeaderFrame::Redirect(
        "https://example.com/foo.js".to_string(),
        "https://example.com/bar.js".to_string()
      )
    );
    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(frame.kind(), HeaderFrameKind::Module);
    assert_eq!(&data[frame.source_range().unwrap()], b"source");
    assert_eq!(
      frame,
      HeaderFrame::Module(
        "https://example.com/foo.js".to_string(),
        DataPointer(0, 6),
        DataPointer(0, 10),
        ModuleKind::JavaScript
      )
    );
    assert_eq!(
      codec.decode(&mut buf).unwrap(),
      Some(HeaderFrame::Redirect(
        "https://example.com/baz.js".to_string(),
        "https://example.com/qux.js".to_string()
      ))
    );
    assert_eq!(
      codec.decode(&mut buf).unwrap(),
      Some(HeaderFrame::Module(
        "https://example.com/bar.js".to_string(),
        DataPointer(6, 7),
        DataPointer(10, 11),
        ModuleKind::JavaScript
      ))
    );
    assert_eq!(codec.decode(&mut buf).unwrap(), None);
  }
}
