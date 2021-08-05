use std::{
  collections::HashMap,
  ops::Not,
  sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
  },
};

use anyhow::Result;
use async_trait::async_trait;
use eetf::{Atom, Pid, Term};
use parking_lot::RwLock;
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;

use crate::message::{EdnMessage, NormalMessage};

pub struct EdnPool {
  node_fullname: String,
  local_pid_counter: AtomicU64,
  creation: u32,
  service: Box<dyn EdnService>,
  dispatch_registry: RwLock<HashMap<String, UnboundedSender<EdnMessage>>>,
}

#[derive(Error, Debug)]
pub enum DispatchError {
  #[error("target pid not available")]
  PidNotAvailable,
}

pub struct DispatchRegistration {
  pool: Arc<EdnPool>,
  key: String,
}

impl Drop for DispatchRegistration {
  fn drop(&mut self) {
    self.pool.dispatch_registry.write().remove(&self.key);
  }
}

#[async_trait]
pub trait EdnService: Send + Sync {
  async fn on_reg_send(
    &self,
    pool: &Arc<EdnPool>,
    from_pid: &Pid,
    to_name: &Atom,
    msg: Term,
  ) -> Result<()>;
}

impl EdnPool {
  pub(crate) fn new(
    node_fullname: String,
    creation: u32,
    service: Box<dyn EdnService>,
  ) -> Arc<Self> {
    Arc::new(Self {
      node_fullname,
      local_pid_counter: AtomicU64::new(0),
      creation,
      service,
      dispatch_registry: RwLock::new(HashMap::new()),
    })
  }

  pub fn service(&self) -> &dyn EdnService {
    &*self.service
  }

  pub fn full_name(&self) -> &str {
    &self.node_fullname
  }

  pub(crate) fn register_dispatch(
    self: &Arc<Self>,
    key: String,
    sender: UnboundedSender<EdnMessage>,
  ) -> Option<DispatchRegistration> {
    let mut reg = self.dispatch_registry.write();
    reg.contains_key(&key).not().then(|| {
      reg.insert(key.clone(), sender);
      DispatchRegistration {
        pool: self.clone(),
        key,
      }
    })
  }

  pub async fn erl_send(&self, to_pid: Pid, term: Term) -> Result<()> {
    let target = self
      .dispatch(&to_pid.node.name)
      .ok_or_else(|| DispatchError::PidNotAvailable)?;
    target
      .send(EdnMessage::Normal(NormalMessage::Send {
        to_pid,
        message: term,
      }))
      .map_err(|_| DispatchError::PidNotAvailable)?;
    Ok(())
  }

  fn dispatch(&self, key: &str) -> Option<UnboundedSender<EdnMessage>> {
    self.dispatch_registry.read().get(key).cloned()
  }

  pub fn generate_pid(&self) -> Pid {
    let value = self.local_pid_counter.fetch_add(1, Ordering::Relaxed);
    let id = (value & 0xffff_ffff) as u32;
    let serial = (value >> 32) as u32;
    Pid {
      node: Atom {
        name: self.node_fullname.clone(),
      },
      id,
      serial,
      creation: self.creation,
    }
  }
}
