use std::{
    cell::RefCell,
    collections::HashSet,
};
use os_pipe::pipe;
use std::os::unix::io::AsRawFd;
use derive_new::new;
use wayland_client::{ protocol::{wl_seat::WlSeat}, NewProxy };
use wayland_protocols::wlr::unstable::data_control::v1::client::{
    zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    *,
};

use crate::seat_data::SeatData;

#[derive(new)]
pub struct DataDeviceHandler {
    seat: WlSeat,
    primary_selection: bool
}

impl DataDeviceHandler {
    fn selection(&mut self, offer_option: Option<ZwlrDataControlOfferV1>) {
        match offer_option {
            None => (),
            Some(offer) => {
                let (read_pipe, write_pipe) = pipe().unwrap();
                let mut seat_data = self.seat.as_ref().user_data::<RefCell<SeatData>>().unwrap().borrow_mut();

                // Replace the existing offer with the new one.
                seat_data.set_offer(Some(offer.clone()));

                // send read pipe to thread reading the content
                seat_data.sender.as_mut().unwrap().borrow_mut().send(read_pipe).unwrap();

                // send receive request for the content type
                offer.receive("text/plain".to_owned(), write_pipe.as_raw_fd());
            }
        }
    }
}

impl zwlr_data_control_device_v1::EventHandler for DataDeviceHandler {
    fn data_offer(&mut self, _device: ZwlrDataControlDeviceV1, offer: NewProxy<ZwlrDataControlOfferV1>) {
        // Make a container for the new offer's mime types.
        let mime_types = RefCell::new(HashSet::<String>::with_capacity(1));
        // Bind the new offer with a handler that fills out mime types.
        offer.implement(DataControlOfferHandler, mime_types);
    }

    fn selection(&mut self, _device: ZwlrDataControlDeviceV1, offer: Option<ZwlrDataControlOfferV1>) {
        self.selection(offer);
    }

    fn primary_selection(&mut self, _device: ZwlrDataControlDeviceV1, offer: Option<ZwlrDataControlOfferV1>) {
        if self.primary_selection {
            self.selection(offer);
        }
    }

    fn finished(&mut self, _device: ZwlrDataControlDeviceV1) {
        // Destroy the device stored in the seat as it's no longer valid.
        let seat_data = self.seat.as_ref().user_data::<RefCell<SeatData>>().unwrap();
        seat_data.borrow_mut().set_device(None);
    }
}

pub struct DataControlOfferHandler;

impl zwlr_data_control_offer_v1::EventHandler for DataControlOfferHandler {
    fn offer(&mut self, offer: ZwlrDataControlOfferV1, mime_type: String) {
        let mime_types = offer.as_ref().user_data::<RefCell<HashSet<_>>>().unwrap();
        mime_types.borrow_mut().insert(mime_type);
    }
}
