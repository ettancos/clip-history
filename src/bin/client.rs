use std::os::unix::net::{UnixStream};
use std::io::prelude::*;
use std::io::BufReader;
use std::rc::Rc;

fn main() {
  let stream = Rc::new(UnixStream::connect("/tmp/clipboard.sock").unwrap());
  let mut reader = BufReader::new(stream.as_ref());

  stream.clone().as_ref().write_all(b"hello\n").unwrap();
  let mut buf = String::new();
  reader.read_line(&mut buf).unwrap();
  println!("{}", buf);

}
