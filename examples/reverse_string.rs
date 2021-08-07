use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use edp::{
  edn_pool::{EdnPool, EdnService},
  edn_server::{EdnServer, EdnServerOpt},
  project_term,
};
use eetf::{Atom, Binary, Pid, Term};

struct Service {}

#[async_trait]
impl EdnService for Service {
  async fn on_reg_send(
    &self,
    pool: &Arc<EdnPool>,
    from_pid: &Pid,
    _to_name: &Atom,
    msg: Term,
  ) -> Result<()> {
    let s = String::from_utf8(project_term!(msg, Binary)?.bytes)?
      .chars()
      .rev()
      .collect::<String>();
    pool
      .erl_send(from_pid.clone(), Term::Binary(Binary { bytes: s.into() }))
      .await?;
    Ok(())
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  if std::env::var("RUST_LOG").is_err() {
    std::env::set_var("RUST_LOG", "debug");
  }

  pretty_env_logger::init_timed();

  let server_opt = EdnServerOpt::builder()
    .node_name("edp-example-reverse-string".into())
    .listen_addr("0.0.0.0:0".into())
    .service(Box::new(Service {}))
    .build();
  let server = EdnServer::start(server_opt).await?;
  match server.run().await? {}
}
