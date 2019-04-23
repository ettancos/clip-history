#[macro_use]
#[deny(unsafe_code)]
extern crate wayland_client;
#[macro_use]
extern crate log;
extern crate smithay_client_toolkit as sctk;
extern crate wayland_protocols;

use pipe_channel;
use os_pipe::PipeReader;
use std::io::prelude::*;
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    env,
    fs::{metadata, remove_file},
    io::BufReader,
    io::Read,
    rc::Rc,
    sync::{mpsc::{channel, Receiver}, Arc, Mutex, MutexGuard},
    vec::Vec,
};

use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat};
use wayland_client::{Display, GlobalError, GlobalManager};
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_device_v1::ZwlrDataControlDeviceV1;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
//use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_source_v1::ZwlrDataControlSourceV1;

use sctk::Environment;
use serde_json::{Result as SerdeResult, Value};

mod seat_data;
use crate::seat_data::SeatData;
mod handlers;
use crate::handlers::DataDeviceHandler;

pub struct WlSeatHandler;

impl wl_seat::EventHandler for WlSeatHandler {
    fn name(&mut self, seat: wl_seat::WlSeat, name: String) {
        let data = seat.as_ref().user_data::<RefCell<SeatData>>().unwrap();
        data.borrow_mut().set_name(name);
    }
}

event_enum!(
    Events |
    Pointer => wl_pointer::WlPointer,
    Keyboard => wl_keyboard::WlKeyboard
);

fn instantiate_data_control_manager(
    globals: &GlobalManager,
    supports_primary: &Rc<Cell<bool>>,
) -> Result<ZwlrDataControlManagerV1, GlobalError> {
    return match globals
        .instantiate_exact::<ZwlrDataControlManagerV1, _>(2, |manager| manager.implement_dummy())
    {
        Ok(manager) => {
            supports_primary.set(true);
            trace!("Using ZwlrDataControlManagerV1 version 2");
            return Ok(manager);
        },
        Err(GlobalError::VersionTooLow(_)) => {
            trace!("Trying ZwlrDataControlManagerV1 version 1");
            globals.instantiate_exact::<ZwlrDataControlManagerV1, _>(1, |manager| {
                manager.implement_dummy()
            })
        },
        Err(GlobalError::Missing) => Err(GlobalError::Missing),
    };
}


fn socket_consumer(history: Arc<Mutex<VecDeque<String>>>) {
    let socket = "/tmp/clipboard.sock";
    match metadata(socket) {
        Err(_) => (),
        Ok(_) => remove_file(socket).unwrap(),
    }

    let listener = UnixListener::bind(socket).unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(socket) => {
                let thread_history = history.clone();
                let socket_ptr = Arc::new(socket);
                let thread_socket = socket_ptr.clone();
                thread::spawn(|| handle_socket(thread_socket, thread_history))
            },
            Err(_) => break,
        };
    }
}

fn handle_socket(stream: Arc<UnixStream>, history: Arc<Mutex<VecDeque<String>>>) {
    let reader = BufReader::new(stream.as_ref());
    for line in reader.lines().map(|l| l.unwrap()) {
        debug!("Socket {}", line);
        if line != "gimme" {
            trace!("Request is bs");
            continue;
        }

        let deque: MutexGuard<VecDeque<String>> = history.lock().unwrap();
        let cloned: VecDeque<String> = deque.clone();
        let serialized = serde_json::to_string(&cloned).unwrap();
        trace!("Socket Serialized: {}", serialized);

        stream.as_ref().write_all(serialized.as_bytes()).unwrap();
        stream.as_ref().write_all("\n".as_bytes()).unwrap();
        stream.as_ref().flush().unwrap();
    }
}

fn content_fetcher(rx: Arc<Mutex<Receiver<PipeReader>>>, history: Arc<Mutex<VecDeque<String>>>) {
    for mut reader in rx.lock().unwrap().iter() {
        let mut buf = vec![];
        reader.read_to_end(&mut buf).unwrap();

        let content = std::str::from_utf8(buf.as_mut()).unwrap().trim().to_owned();
        debug!("Clipboard content {}", content);
        // skip blank strings for convinience
        if content.is_empty() {
            continue;
        }

        let mut deque = history.lock().unwrap();
        let mut filtered: VecDeque<String> = deque.iter().map(|s| s.to_owned()).filter(|s| *s != content).collect();
        filtered.push_back(content);
        if filtered.len() > 100 {
            filtered.pop_front();
        }
        deque.clear();
        deque.append(&mut filtered);
    }
}
fn initialize() -> (wayland_client::EventQueue, Rc<RefCell<Vec<wl_seat::WlSeat>>>, wayland_client::GlobalManager) {
    let (display, mut event_queue) = Display::connect_to_env()
        .expect("Failed to connect to the Wayland server.");
    let _env = Environment::from_display(&display, &mut event_queue).unwrap();

    let seats = Rc::new(RefCell::new(Vec::<wl_seat::WlSeat>::new()));
    let seats_2 = seats.clone();

    let globals = GlobalManager::new_with_cb(
        &display,
        global_filter!([
            wl_seat::WlSeat,
            6,
            move |seat: NewProxy<wl_seat::WlSeat>| {
                let seat_data = RefCell::new(SeatData::default());
                let seat = seat.implement(WlSeatHandler, seat_data);
                seats_2.borrow_mut().push(seat.clone());
                seat
            }
        ]),
    );
    event_queue.sync_roundtrip().unwrap();
    if seats.borrow().is_empty() {
        panic!("No seat");
    }

    return (event_queue, seats, globals);
}

fn main() {
    let (mut event_queue, seats, globals) = initialize();
    let supports_primary = Rc::new(Cell::new(false));

    // Try v2 with falling back to v1 or panic if zwlr_data_control_manager_v1 is not supported by the compositor
    let clipboard = instantiate_data_control_manager(&globals.clone(), &supports_primary).unwrap();

    let mut data_devices = Vec::<ZwlrDataControlDeviceV1>::new();

    let mut threads = vec![];
    // Go through the seats and get their data devices.
    //let (spipe, rpipe) = pipe_channel::channel();

    let (sender, receiver) = channel::<PipeReader>();
    let tx = Rc::new(RefCell::new(sender));
    let rx = Arc::new(Mutex::new(receiver));

    let history = Arc::new(Mutex::new(VecDeque::<String>::new()));

    for seat in &*seats.borrow() {
        seat.as_ref()
            .user_data::<RefCell<SeatData>>()
            .unwrap()
            .borrow_mut()
            .set_sender(Some(tx.clone()));

        let rx_clone = rx.clone();
        let history_clone = history.clone();

        threads.push(thread::spawn(move || {
            content_fetcher(rx_clone, history_clone);
        }));

        let handler = DataDeviceHandler::new(seat.clone(), false);
        let device = clipboard
            .get_data_device(seat, |device| device.implement(handler, ()))
            .unwrap();
        data_devices.push(device);
    }

    thread::spawn(move || {
        socket_consumer(history);
    });

    loop {
        event_queue.dispatch().unwrap();
    }
}
