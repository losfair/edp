use anyhow::Result;
use std::convert::TryFrom;
use thiserror::Error;
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter},
  net::{TcpStream, ToSocketAddrs},
};
use typed_builder::TypedBuilder;

use crate::protocol::ProtocolError;

pub const NODE_TYPE_NORMAL: u8 = 77;
pub const NODE_TYPE_HIDDEN: u8 = 72;

#[derive(Debug, Error)]
pub enum EpmdClientError {
  #[error("registration failed")]
  RegistrationFailed,
}

pub struct EpmdClient<S> {
  stream: BufReader<S>,
}

#[derive(TypedBuilder, Clone, Debug)]
pub struct EpmdRegistration {
  port: u16,

  #[builder(default = NODE_TYPE_HIDDEN)]
  node_type: u8,

  #[builder(default = 0)]
  protocol: u8,

  #[builder(default = 6)]
  highest_version: u16,

  #[builder(default = 6)]
  lowest_version: u16,

  node_name: String,

  #[builder(default)]
  extra: Vec<u8>,
}

impl EpmdClient<TcpStream> {
  pub async fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self> {
    let stream = TcpStream::connect(addr).await?;
    Ok(Self::new(stream))
  }
}

impl<S: AsyncRead + AsyncWrite + Unpin> EpmdClient<S> {
  pub fn new(stream: S) -> Self {
    Self {
      stream: BufReader::new(stream),
    }
  }

  /// https://erlang.org/doc/apps/erts/erl_dist_protocol.html#register-a-node-in-epmd
  pub async fn register_node(&mut self, reg: &EpmdRegistration) -> Result<u32> {
    let mut buf: Vec<u8> = vec![];

    {
      use byteorder::{BigEndian, WriteBytesExt};
      WriteBytesExt::write_u8(&mut buf, 120)?;
      WriteBytesExt::write_u16::<BigEndian>(&mut buf, reg.port)?;
      WriteBytesExt::write_u8(&mut buf, reg.node_type)?;
      WriteBytesExt::write_u8(&mut buf, reg.protocol)?;
      WriteBytesExt::write_u16::<BigEndian>(&mut buf, reg.highest_version)?;
      WriteBytesExt::write_u16::<BigEndian>(&mut buf, reg.lowest_version)?;

      let name_bytes = reg.node_name.as_bytes();
      WriteBytesExt::write_u16::<BigEndian>(&mut buf, u16::try_from(name_bytes.len())?)?;
      buf.extend_from_slice(name_bytes);

      WriteBytesExt::write_u16::<BigEndian>(&mut buf, u16::try_from(reg.extra.len())?)?;
      buf.extend_from_slice(&reg.extra);
    }

    let mut w = BufWriter::new(&mut self.stream);
    w.write_u16(u16::try_from(buf.len())?).await?;
    w.write_all(&buf).await?;
    w.flush().await?;
    drop(w);

    let first_byte = self.stream.read_u8().await?;
    let result = self.stream.read_u8().await?;

    // creation
    let creation = if first_byte == 118 {
      // ALIVE2_X_RESP
      self.stream.read_u32().await?
    } else if first_byte == 121 {
      // ALIVE2_RESP
      self.stream.read_u16().await? as u32
    } else {
      return Err(ProtocolError.into());
    };

    if result != 0 {
      return Err(EpmdClientError::RegistrationFailed.into());
    }

    log::debug!("creation: {}", creation);

    Ok(creation)
  }
}
