use thiserror::Error;

#[derive(Error, Debug)]
pub enum TermProjectError {
  #[error("failed to project term `{0}` to variant `{1}`")]
  Failure(String, &'static str),
}
