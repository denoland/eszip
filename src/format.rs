use bytes::Buf;
use bytes::BytesMut;
use std::convert::TryFrom;
use tokio_util::codec::Decoder;

macro_rules! repr_u8 {
  ($(#[$meta:meta])* $vis:vis enum $name:ident {
    $($(#[$vmeta:meta])* $vname:ident $(= $val:expr)?,)*
  }) => {
    $(#[$meta])*
    $vis enum $name {
      $($(#[$vmeta])* $vname $(= $val)?,)*
    }

    impl std::convert::TryFrom<u8> for $name {
      type Error = ();

      fn try_from(v: u8) -> Result<Self, Self::Error> {
          match v {
              $(x if x == $name::$vname as u8 => Ok($name::$vname),)*
              _ => Err(()),
          }
      }
    }
  }
}

#[derive(Debug, PartialEq)]
pub enum HeaderFrame {
  // specifier => (offset, length) pointer to data section
  Module(String, usize, usize),
  // specifier => specifier
  Redirect(String, String),
}

repr_u8! {
  #[repr(u8)]
  pub enum HeaderFrameKind {
    Module = 0,
    Redirect = 1,
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
      _ => Ok(None),
    }
  }
}

impl HeaderFrame {
  pub fn kind(&self) -> HeaderFrameKind {
    match self {
      HeaderFrame::Module(_, _, _) => HeaderFrameKind::Module,
      HeaderFrame::Redirect(_, _) => HeaderFrameKind::Redirect,
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
