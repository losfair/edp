#[macro_export]
macro_rules! project_term {
  ($x:expr, $v:ident) => {{
    let x = $x;
    match x {
      $crate::eetf::Term::$v(value) => Ok(value),
      _ => Err($crate::term::TermProjectError::Failure(
        format!("{:?}", x),
        stringify!($v),
      )),
    }
  }};
}
