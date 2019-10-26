use rusb::UsbContext;
use crate::device_setup::DeviceReceiver;
use std::sync::{Arc, Mutex};

mod device_setup;

fn main() {
	let context = rusb::Context::new().unwrap();

	let device_receiver = Arc::new(Mutex::new(DeviceReceiver::new()));

	let usb_listener = USBListener::new(device_receiver.clone());

	let _ = context.register_callback(Some(0x256c), Some(0x006e), None, Box::new(usb_listener));

	start_current_plugged_devices(&mut device_receiver.lock().unwrap(), &context);

	loop {
		context.handle_events(None).unwrap();
	}
}

fn start_current_plugged_devices (receiver: &mut DeviceReceiver<rusb::Context>, context: &rusb::Context) {
	let devices_iterator = context.devices().unwrap();

	let mut matched_devices: Vec<_> = devices_iterator
	.iter()
	.filter(is_device_huion_tablet)
	.collect();

	while let Some(device) = matched_devices.pop() {
		if is_device_huion_tablet(&device) {
			receiver.add_device(device);
		}
	}
}

struct USBListener <T: 'static +  rusb::UsbContext> {
	device_handler: Arc<Mutex<DeviceReceiver<T>>>
}

impl <T: 'static +  rusb::UsbContext> USBListener<T> {
	fn new(device_receiver: Arc<Mutex<DeviceReceiver<T>>>) -> USBListener<T> {
		USBListener {
			device_handler: device_receiver
		}
	}
}

impl <T: 'static +  rusb::UsbContext> rusb::Hotplug<T> for USBListener<T> {
	fn device_arrived(&mut self, device: rusb::Device<T>) {
		if is_device_huion_tablet(&device) {
			return;
		}

		self.device_handler.lock().unwrap().add_device(device);
	}

	fn device_left(&mut self, _device: rusb::Device<T>) {
		unimplemented!()
	}
}

fn is_device_huion_tablet<T: rusb::UsbContext>(device: &rusb::Device<T>) -> bool {
	let device_descriptor = device.device_descriptor().unwrap();
	device_descriptor.vendor_id() == 0x256c && device_descriptor.product_id() == 0x006e
}
