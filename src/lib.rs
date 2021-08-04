#[macro_use]
pub mod macros;
pub mod decode_util;
mod edn_connection;
pub mod edn_pool;
pub mod edn_server;
pub mod epmd_client;
mod message;
pub mod protocol;
pub mod term;

pub use eetf;

#[cfg(test)]
mod tests {
  #[test]
  fn it_works() {
    assert_eq!(2 + 2, 4);
  }
}

pub enum Never {}
