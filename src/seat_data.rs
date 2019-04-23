use std::rc::Rc;
use std::cell::RefCell;
use std::sync::mpsc::Sender;
use os_pipe::PipeReader;

use wayland_protocols::wlr::unstable::data_control::v1::client::{
    zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
};
 
#[derive(Default)]
pub struct SeatData {
    /// The name of this seat, if any.
    pub name: Option<String>,

    /// The data device of this seat, if any.
    pub device: Option<ZwlrDataControlDeviceV1>,

    /// The data offer of this seat, if any.
    ///
    pub offer: Option<ZwlrDataControlOfferV1>,

    pub sender: Option<Rc<RefCell<Sender<PipeReader>>>>
}

impl SeatData {
    /// Sets this seat's name.
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    pub fn set_sender(&mut self, sender: Option<Rc<RefCell<Sender<PipeReader>>>>) {
        self.sender = sender;
    }

    /// Sets this seat's device.
    ///
    /// Destroys the old one, if any.
    pub fn set_device(&mut self, device: Option<ZwlrDataControlDeviceV1>) {
        let old_device = self.device.take();
        self.device = device;

        if let Some(device) = old_device {
            device.destroy();
        }
    }

    /// Sets this seat's data offer.
    ///
    /// Destroys the old one, if any.
    pub fn set_offer(&mut self, new_offer: Option<ZwlrDataControlOfferV1>) {
        let old_offer = self.offer.take();
        self.offer = new_offer;

        if let Some(offer) = old_offer {
            offer.destroy();
        }
    }
}
