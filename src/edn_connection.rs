//! Erlang Distributed Node process.

use crate::{
  decode_util::{read_16bit_be_length_prefixed_string, TermIndex},
  edn_pool::EdnPool,
  message::EdnMessage,
  protocol::{DistFlags, ProtocolError},
  Never,
};
use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use eetf::{Term, Tuple};
use md5::{Digest, Md5};
use rand::Rng;
use std::{convert::TryFrom, ops::Not, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter},
  sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot,
  },
  time::sleep,
};
use typed_builder::TypedBuilder;

pub struct EdnConnection<R, W> {
  stream_r: BufReader<R>,
  stream_w: Option<BufWriter<W>>,
  local_name: PeerName,
  creation: u32,
  cookie: String,
  tx: UnboundedSender<EdnMessage>,
  rx: Option<UnboundedReceiver<EdnMessage>>,
}

#[derive(TypedBuilder)]
pub struct EdnConnectionOpt {
  name: String,
  cookie: String,
  creation: u32,
  send_buffer_size: usize,
  recv_buffer_size: usize,
}

#[derive(Error, Debug)]
pub enum EdnError {
  #[error("auth error")]
  AuthError,

  #[error("message too large")]
  MessageTooLarge,

  #[error("connection timed out")]
  Timeout,

  #[error("not in handshake state")]
  NotInHandshake,

  #[error("send channel not available")]
  SendNotAvailable,
}

#[derive(Debug)]
pub struct PeerName {
  pub name: String,
  pub flags: DistFlags,
}

impl<R: AsyncRead + Send + Unpin, W: AsyncWrite + Send + Unpin + 'static> EdnConnection<R, W> {
  pub(crate) fn new(
    stream_r: R,
    stream_w: W,
    opt: EdnConnectionOpt,
  ) -> (Self, UnboundedSender<EdnMessage>) {
    let (tx, rx) = unbounded_channel();
    (
      Self {
        stream_r: BufReader::with_capacity(opt.recv_buffer_size, stream_r),
        stream_w: Some(BufWriter::with_capacity(opt.send_buffer_size, stream_w)),
        local_name: PeerName {
          name: opt.name,
          flags: DistFlags::default(),
        },
        creation: opt.creation,
        cookie: opt.cookie,
        tx: tx.clone(),
        rx: Some(rx),
      },
      tx,
    )
  }

  async fn flush_handshake(&mut self) -> Result<()> {
    self
      .stream_w
      .as_mut()
      .ok_or_else(|| EdnError::NotInHandshake)?
      .flush()
      .await?;
    Ok(())
  }

  async fn write_handshake_packet(&mut self, pkt: &[u8]) -> Result<()> {
    let stream_w = self
      .stream_w
      .as_mut()
      .ok_or_else(|| EdnError::NotInHandshake)?;
    stream_w.write_u16(u16::try_from(pkt.len())?).await?;
    stream_w.write_all(pkt).await?;
    Ok(())
  }

  async fn read_handshake_packet(&mut self) -> Result<Vec<u8>> {
    let len = self.stream_r.read_u16().await? as usize;
    let mut buf = vec![0u8; len];
    self.stream_r.read_exact(&mut buf).await?;
    Ok(buf)
  }

  async fn send_challenge(&mut self, challenge: u32) -> Result<()> {
    let mut pkt: Vec<u8> = Vec::new();

    WriteBytesExt::write_u8(&mut pkt, b'N')?;
    WriteBytesExt::write_u64::<BigEndian>(&mut pkt, self.local_name.flags.bits())?;
    WriteBytesExt::write_u32::<BigEndian>(&mut pkt, challenge)?;
    WriteBytesExt::write_u32::<BigEndian>(&mut pkt, self.creation)?;

    let name = self.local_name.name.as_bytes();
    WriteBytesExt::write_u16::<BigEndian>(&mut pkt, u16::try_from(name.len())?)?;
    pkt.extend_from_slice(name);
    self.write_handshake_packet(&pkt).await?;

    Ok(())
  }

  fn auth_digest(&self, challenge: u32) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(format!("{}{}", self.cookie, challenge).as_bytes());
    hasher.finalize().into()
  }

  async fn receive_and_validate_challenge_reply(&mut self, our_challenge: u32) -> Result<u32> {
    let msg = self.read_handshake_packet().await?;
    let r = &mut msg.as_slice();
    if ReadBytesExt::read_u8(r)? != b'r' {
      return Err(ProtocolError.into());
    }
    let their_challenge = ReadBytesExt::read_u32::<BigEndian>(r)?;

    let expected_digest = self.auth_digest(our_challenge);
    if *r != &expected_digest {
      log::error!(
        "auth error: expecting {}, got {}",
        hex::encode(&expected_digest),
        hex::encode(r)
      );
      return Err(EdnError::AuthError.into());
    }

    Ok(their_challenge)
  }

  async fn compute_and_send_challenge_ack(&mut self, their_challenge: u32) -> Result<()> {
    let digest = self.auth_digest(their_challenge);
    let mut buf: [u8; 17] = [0u8; 17];
    buf[0] = b'a';
    buf[1..].copy_from_slice(&digest);
    self.write_handshake_packet(&buf).await?;
    Ok(())
  }

  async fn receive_name(&mut self) -> Result<PeerName> {
    let msg = self.read_handshake_packet().await?;
    let r = &mut msg.as_slice();

    let head = ReadBytesExt::read_u8(r)?;
    let flags: DistFlags;
    let name: String;
    if head == b'N' {
      // Protocol version 6

      flags = DistFlags::from_bits_truncate(ReadBytesExt::read_u64::<BigEndian>(r)?);
      if !flags.contains(DistFlags::DFLAG_HANDSHAKE_23) {
        log::error!("peer does not support DFLAG_HANDSHAKE_23");
        return Err(ProtocolError.into());
      }
      if !flags.contains(DistFlags::DFLAG_V4_NC) {
        log::error!("peer does not support DFLAG_V4_NC");
        return Err(ProtocolError.into());
      }

      let _creation = ReadBytesExt::read_u32::<BigEndian>(r)?;
      name = read_16bit_be_length_prefixed_string(r)?;
    } else {
      return Err(ProtocolError.into());
    }
    Ok(PeerName { name, flags })
  }

  pub(crate) async fn server_handshake(&mut self) -> Result<PeerName> {
    let peer_name = self.receive_name().await?;
    self.write_handshake_packet(&[b's', b'o', b'k']).await?;

    let our_challenge: u32 = rand::thread_rng().gen();
    self.send_challenge(our_challenge).await?;
    self.flush_handshake().await?;

    let their_challenge = self
      .receive_and_validate_challenge_reply(our_challenge)
      .await?;
    self.compute_and_send_challenge_ack(their_challenge).await?;
    self.flush_handshake().await?;

    Ok(peer_name)
  }

  async fn write_worker_inner(
    mut stream_w: BufWriter<W>,
    mut rx: UnboundedReceiver<EdnMessage>,
  ) -> Result<()> {
    let mut dirty = false;
    loop {
      let msg = tokio::select! {
        biased;

        x = rx.recv() => {
          x
        },
        _ = futures::future::ready(()), if dirty => {
          dirty = false;
          stream_w.flush().await?;
          continue;
        },
      };
      let msg = match msg {
        Some(x) => x,
        None => return Ok(()),
      };
      let bytes = msg.to_bytes()?;
      stream_w.write_u32(u32::try_from(bytes.len())?).await?;
      stream_w.write_all(&bytes).await?;
      dirty = true;
    }
  }

  async fn write_worker(
    stream_w: BufWriter<W>,
    rx: UnboundedReceiver<EdnMessage>,
    close: oneshot::Receiver<()>,
  ) {
    tokio::select! {
      res = Self::write_worker_inner(stream_w, rx) => {
        if let Err(e) = res {
          log::error!("write worker error: {:?}", e);
        }
      }
      _ = close => {

      }
    }
  }

  async fn dispatch_reg_send(pool: Arc<EdnPool>, control: Tuple, message: Term) -> Result<()> {
    let from_pid = project_term!(control.elements.term_index(1)?, Pid)?;
    let to_name = project_term!(control.elements.term_index(3)?, Atom)?;
    pool
      .service()
      .on_reg_send(&pool, from_pid, to_name, message)
      .await
  }

  pub(crate) async fn dispatch_loop(&mut self, pool: Arc<EdnPool>) -> Result<Never> {
    let (stream_w, rx) = self
      .stream_w
      .take()
      .and_then(|x| self.rx.take().map(|y| (x, y)))
      .ok_or_else(|| ProtocolError)?;
    let (_close_tx, close_rx) = oneshot::channel();
    tokio::spawn(async move {
      Self::write_worker(stream_w, rx, close_rx).await;
    });
    loop {
      let (control, message) = self.receive_message().await?;
      log::debug!("control: {:?}", control);

      let control = project_term!(control, Tuple)?;
      let op_type = project_term!(control.elements.term_index(0)?, FixInteger)?.value;
      match op_type {
        6 => {
          // REG_SEND
          let pool = pool.clone();
          let message = message.ok_or_else(|| ProtocolError)?;
          tokio::spawn(async move {
            match Self::dispatch_reg_send(pool, control, message).await {
              Ok(()) => {}
              Err(e) => {
                log::error!("dispatch_reg_send error: {:?}", e);
              }
            }
          });
        }
        _ => {
          log::warn!("ignoring unknown operation: {}", op_type);
        }
      }
    }
  }

  fn send_message(&mut self, msg: EdnMessage) -> Result<()> {
    self
      .tx
      .send(msg)
      .map_err(|_| EdnError::SendNotAvailable.into())
  }

  async fn raw_receive(&mut self) -> Result<Option<Vec<u8>>> {
    let len = self.stream_r.read_u32().await? as usize;
    if len == 0 {
      // Tick
      self.send_message(EdnMessage::TickReply)?;
      return Ok(None);
    }
    if len > 1048576 * 4 {
      return Err(EdnError::MessageTooLarge.into());
    }

    let mut msg = vec![0u8; len];
    self.stream_r.read_exact(&mut msg).await?;
    Ok(Some(msg))
  }

  async fn receive_message(&mut self) -> Result<(Term, Option<Term>)> {
    let msg = loop {
      tokio::select! {
        x = self.raw_receive() => {
          match x? {
            Some(x) => break x,
            None => continue,
          }
        }
        _ = sleep(Duration::from_secs(60)) => {
          return Err(EdnError::Timeout.into());
        }
      }
    };
    let mut reader = msg.as_slice();

    // XXX: https://erlang.org/doc/apps/erts/erl_dist_protocol.html#protocol-between-connected-nodes
    //
    // > Since ERTS 5.7.2 (OTP R13B) the runtime system passes a distribution flag in the handshake stage
    // that enables the use of a distribution header on all messages passed.
    // >
    // > Nodes with an ERTS version earlier than 5.7.2 (OTP R13B) does not pass the distribution flag that
    // enables the distribution header.
    //
    // Seems that we are not getting the distribution header?
    if ReadBytesExt::read_u8(&mut reader)? != 112 {
      return Err(ProtocolError.into());
    }

    let control = Term::decode(&mut reader)?;
    let message = reader
      .is_empty()
      .not()
      .then(|| Term::decode(reader))
      .transpose()?;
    Ok((control, message))
  }
}
