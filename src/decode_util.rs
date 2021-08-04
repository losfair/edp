use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Read;
use thiserror::Error;

pub fn read_16bit_be_length_prefixed_bytes<R: Read>(x: &mut R) -> Result<Vec<u8>> {
  let len = x.read_u16::<BigEndian>()?;
  let mut buf = vec![0u8; len as usize];
  x.read_exact(&mut buf)?;
  Ok(buf)
}

pub fn read_16bit_be_length_prefixed_string<R: Read>(x: &mut R) -> Result<String> {
  read_16bit_be_length_prefixed_bytes(x)
    .and_then(|x| String::from_utf8(x).map_err(anyhow::Error::from))
}

#[derive(Error, Debug)]
#[error("term index out of bounds")]
pub struct TermIndexOob;

pub trait TermIndex {
  type Target;
  fn term_index(&self, index: usize) -> Result<&Self::Target, TermIndexOob>;
}

impl<T> TermIndex for [T] {
  type Target = T;
  fn term_index(&self, index: usize) -> Result<&Self::Target, TermIndexOob> {
    self.get(index).ok_or_else(|| TermIndexOob)
  }
}
