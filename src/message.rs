use anyhow::Result;
use eetf::{Atom, FixInteger, Pid, Term, Tuple};

#[derive(Clone, Debug)]
pub(crate) enum EdnMessage {
  TickReply,
  Normal(NormalMessage),
}

#[derive(Clone, Debug)]
pub(crate) enum NormalMessage {
  Send { to_pid: Pid, message: Term },
}

impl EdnMessage {
  pub fn to_bytes(&self) -> Result<Vec<u8>> {
    Ok(match self {
      Self::TickReply => vec![],
      Self::Normal(x) => {
        let mut buf = vec![112u8];
        match x {
          NormalMessage::Send { to_pid, message } => {
            let control = Term::Tuple(Tuple {
              elements: vec![
                Term::FixInteger(FixInteger { value: 2 }),
                Term::Atom(Atom { name: "".into() }),
                Term::Pid(to_pid.clone()),
              ],
            });
            control.encode(&mut buf)?;
            message.encode(&mut buf)?;
          }
        }
        buf
      }
    })
  }
}
