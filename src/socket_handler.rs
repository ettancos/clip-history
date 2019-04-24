use std::sync::{Mutex, Arc};
use std::collections::VecDeque;
use std::fs::{remove_file, metadata};
use std::os::unix::net::{UnixListener, UnixStream};
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::thread;
use std::io::ErrorKind;
use serde::{Deserialize};
use serde_json;

pub fn from_path(socket_path: &str) -> io::Result<Arc<UnixListener>> {
    match metadata(socket_path) {
      Ok(_) => remove_file(socket_path),
      Err(err) => match err.kind() {
        ErrorKind::NotFound => Ok(()),
        _ => Err(err)
      }
    }?;

    return Ok(Arc::new(UnixListener::bind(socket_path)?));
}

/// Starts a thread with a handler for every incoming connection
pub fn handle_socket_connections(listener: Arc<UnixListener>, history: Arc<Mutex<VecDeque<String>>>) -> io::Result<()> {
    for stream in listener.incoming() {
        match stream {
            Ok(socket) => {
                let thread_history = history.clone();
                let socket_ptr = Arc::new(socket);
                let thread_socket = socket_ptr.clone();
                thread::spawn(|| handle_clipboard_requests(thread_socket, thread_history));
            },
            Err(_) => break,
        };
    }
    return Ok(());
}

#[derive(Deserialize, Debug)]
struct ClipboardHistoryRequest {
  count: usize
}

/// Read requests if its proper return the current status of the clipboard history
fn handle_clipboard_requests(stream: Arc<UnixStream>, history: Arc<Mutex<VecDeque<String>>>) -> io::Result<()> {
    let reader = BufReader::new(stream.as_ref());
    for line in reader.lines().map(|l| l.unwrap()) {
      trace!("Raw request: {}", line);
      let clip: ClipboardHistoryRequest = match serde_json::from_str(&line) {
        Ok(c) => c,
        Err(e) => {
          println!("Invalid JSON: {}", e);
          continue;
        }
      };
      debug!("Parsed request: {:?}", clip);

      let response: VecDeque<String> = history.lock().unwrap().iter().rev().take(clip.count).map(|s| s.clone()).collect();
      let serialized = match serde_json::to_string(&response) {
        Err(err) => {
          println!("Serialization failure: {}", err);
          continue;
        },
        Ok(s) => s
      };
      trace!("Socket Serialized: {}", serialized);
      
      let _: io::Result<()> = match stream.as_ref()
        .write_all(serialized.as_bytes())
        .and_then(|()| stream.as_ref().flush()) {
          Err(err) => match err.kind() {
            ErrorKind::BrokenPipe => Err(err),
            _ => Ok(())
          },
          Ok(_) => Ok(())
        };
    };
    Ok(())
}