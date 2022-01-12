use bytes::Buf;
use bytes::BytesMut;
use std::convert::TryFrom;
use tokio_util::codec::Decoder;

#[derive(Debug, PartialEq)]
struct DataPointer(usize, usize);

#[repr(u8)]
#[derive(Debug, PartialEq)]
pub enum ModuleKind {
  TypeScript,
  JavaScript,
  JSX,
  TSX,
}

impl std::convert::TryFrom<u8> for ModuleKind {
  type Error = ();

  fn try_from(v: u8) -> Result<Self, Self::Error> {
    match v {
      x if x == ModuleKind::TypeScript as u8 => Ok(ModuleKind::TypeScript),
      x if x == ModuleKind::JavaScript as u8 => Ok(ModuleKind::JavaScript),
      x if x == ModuleKind::JSX as u8 => Ok(ModuleKind::JSX),
      x if x == ModuleKind::TSX as u8 => Ok(ModuleKind::TSX),
      _ => Err(()),
    }
  }
}

#[derive(Debug, PartialEq)]
pub enum HeaderFrame {
  // specifier => (offset, length) pointer to data section
  // TODO(@littledivy): move this to a struct
  Module(String, DataPointer, DataPointer, ModuleKind),
  // specifier => specifier
  Redirect(String, String),
}

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

pub struct Reader;

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
impl Decoder for Reader {
  type Item = HeaderFrame;
  type Error = std::io::Error;

  fn decode(
    &mut self,
    buf: &mut BytesMut,
  ) -> Result<Option<Self::Item>, Self::Error> {
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
    match entry_kind {
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

        Ok(Some(HeaderFrame::Redirect(specifier, source)))
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

        Ok(Some(HeaderFrame::Module(
          specifier,
          source_ptr,
          source_map_ptr,
          module_type,
        )))
      }
    }
  }
}

impl HeaderFrame {
  pub fn kind(&self) -> HeaderFrameKind {
    match self {
      HeaderFrame::Module(..) => HeaderFrameKind::Module,
      HeaderFrame::Redirect(..) => HeaderFrameKind::Redirect,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bytes::BufMut;

  fn encode_redirect(specifier: &str) -> BytesMut {
    let mut buf = BytesMut::new();
    let specifier = specifier.as_bytes();

    buf.put_u32(specifier.len() as u32);
    buf.put(specifier);
    buf.put_u8(HeaderFrameKind::Redirect as u8);

    buf.put_u32(specifier.len() as u32);
    buf.put(specifier);
    buf
  }

  #[test]
  fn test_decode() {
    let mut codec = Reader;

    let mut buf = BytesMut::new();
    assert_eq!(codec.decode(&mut buf).unwrap(), None);

    let mut buf = encode_redirect("https://example.com/foo.js");
    assert_eq!(
      codec.decode(&mut buf).unwrap(),
      Some(HeaderFrame::Redirect(
        "https://example.com/foo.js".to_string(),
        "https://example.com/foo.js".to_string()
      ))
    );
  }
}
