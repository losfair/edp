use std::sync::Arc;

use anyhow::Result;
use tokio::{
  net::{lookup_host, TcpListener, TcpStream},
  sync::oneshot,
};
use typed_builder::TypedBuilder;

use crate::{
  edn_connection::{EdnConnection, EdnConnectionOpt},
  edn_pool::{EdnPool, EdnService},
  epmd_client::{EpmdClient, EpmdRegistration},
  Never,
};
use thiserror::Error;

#[derive(TypedBuilder)]
pub struct EdnServerOpt {
  #[builder(default = "127.0.0.1:0".to_string())]
  listen_addr: String,
  node_name: String,

  #[builder(default, setter(strip_option))]
  cookie: Option<String>,
  service: Box<dyn EdnService>,

  #[builder(default = 8192)]
  send_buffer_size: usize,

  #[builder(default = 8192)]
  recv_buffer_size: usize,

  #[builder(default = "127.0.0.1:4369".to_string())]
  epmd_addr: String,
}

pub struct EdnServer {
  epmd_client: EpmdClient<TcpStream>,
  listener: TcpListener,
  creation: u32,
  cookie: String,
  node_name: String,
  hostname: String,
  pool: Arc<EdnPool>,
  send_buffer_size: usize,
  recv_buffer_size: usize,
}

#[derive(Debug, Error)]
pub enum EdnServerError {
  #[error("name resolution failed")]
  NameResolutionFailed,

  #[error("ipv6 is not supported")]
  Ipv6NotSupported,
}

impl EdnServer {
  pub async fn start(opt: EdnServerOpt) -> Result<Self> {
    let listen_addr = lookup_host(&opt.listen_addr)
      .await?
      .next()
      .ok_or_else(|| EdnServerError::NameResolutionFailed)?;

    // Listen and then register so that the clients won't get invalid registrations.
    let listener = TcpListener::bind(listen_addr).await?;
    let actual_addr = listener.local_addr()?;

    let mut epmd_client = EpmdClient::connect(&opt.epmd_addr).await?;
    let reg = EpmdRegistration::builder()
      .node_name(opt.node_name.clone())
      .port(actual_addr.port())
      .build();

    let creation = epmd_client.register_node(&reg).await?;
    log::info!("registered node at {}", actual_addr);

    let hostname = hostname::get().map(|x| x.to_string_lossy().into_owned())?;
    let pool = EdnPool::new(
      format!("{}@{}", opt.node_name, hostname),
      creation,
      opt.service,
    );

    let cookie = if let Some(x) = opt.cookie {
      x
    } else {
      std::fs::read_to_string(&dirs::home_dir().unwrap().join(".erlang.cookie"))?
    };

    Ok(Self {
      epmd_client,
      listener,
      creation,
      cookie,
      node_name: opt.node_name,
      hostname,
      pool,
      send_buffer_size: opt.send_buffer_size,
      recv_buffer_size: opt.recv_buffer_size,
    })
  }

  pub async fn run(self) -> Result<Never> {
    let (_close_tx, close_rx) = oneshot::channel::<()>();
    let (epmd_error_tx, mut epmd_error_rx) = oneshot::channel();
    let mut epmd = self.epmd_client;
    tokio::spawn(async move {
      tokio::select! {
        res = epmd.monitor_connection() => {
          let _ = epmd_error_tx.send(match res {
            Ok(x) => match x {},
            Err(e) => e,
          });
        }
        _ = close_rx => {}
      }
    });
    loop {
      let (conn, _) = tokio::select! {
        x = self.listener.accept() => x?,
        e = &mut epmd_error_rx => return Err(e?),
      };
      let (conn_r, conn_w) = conn.into_split();
      let (mut edn_conn, tx) = EdnConnection::new(
        conn_r,
        conn_w,
        EdnConnectionOpt::builder()
          .name(format!("{}@{}", self.node_name, self.hostname,))
          .cookie(self.cookie.clone())
          .creation(self.creation)
          .send_buffer_size(self.send_buffer_size)
          .recv_buffer_size(self.recv_buffer_size)
          .build(),
      );
      let pool = self.pool.clone();
      tokio::spawn(async move {
        match edn_conn.server_handshake().await {
          Ok(name) => {
            let _reg = pool.register_dispatch(name.name, tx);
            match edn_conn.dispatch_loop(pool).await {
              Ok(x) => match x {},
              Err(e) => {
                log::error!("dispatch_loop failed: {:?}", e);
              }
            }
          }
          Err(e) => {
            log::error!("server_handshake failed: {:?}", e);
          }
        }
      });
    }
  }

  pub fn pool(&self) -> &Arc<EdnPool> {
    &self.pool
  }
}
