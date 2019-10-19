use uinput::event::Event::Controller as uinputController;
use uinput::event::controller::Controller::{Digi as uinputDigi};
use uinput::event::controller::Digi::{Touch as uinputTouch, Pen as uinputPen, Stylus as uinputStylus, Stylus2 as uinputStylus2};
use uinput::event::absolute::Position::{X as uinputPositionX, Y as uinputPositionY};
use uinput::event::absolute::Digi::{Pressure as uinputPressure, TiltX as uinputTiltX, TiltY as uinputTiltY};

fn main() {
	let context = rusb::Context::new().unwrap();

	let devices_iterator = context.devices().unwrap();

	let devices = devices_iterator.iter()
	.filter(|device| {
		let device_descriptor = device.device_descriptor().unwrap();
		device_descriptor.vendor_id() == 0x256c && device_descriptor.product_id() == 0x006e
	});

	let mut matched_devices: Vec<rusb::Device> = devices.collect();

	let device = matched_devices.pop();

	let device = device.expect("No graphic tablet connected found. Check the USB.");

	let device_desc = device.device_descriptor().unwrap();

	println!("Bus {:03} Device {:03} ID {:04x}:{:04x}",
		device.bus_number(),
		device.address(),
		device_desc.vendor_id(),
		device_desc.product_id());

	let handler = device.open();

	if let Err(error) = handler {
		println!("Cannot open the device: {:?}", error);
		return;
	}

	let mut handler = handler.unwrap();

	let config_descriptor = device.active_config_descriptor().unwrap();

	//println!("Active config descriptor: {:?}", config_descriptor);
	println!("Active config descriptor number: {:?}", config_descriptor.number());
	//println!("Config descriptor 0: {:?}", device.config_descriptor(0)); //Why 0? It should be one as active_config_descriptor says!!!???

	println!("Interfaces count: {:?}", config_descriptor.interfaces().count());

	println!("Active config: {:?}", handler.active_configuration());
	
	println!("Set active config: {:?}", handler.set_active_configuration(1));

	let endpoint = detach_kernel(&config_descriptor, &mut handler)[0];

	let mut _system_device = uinput::default().unwrap()
		.name("Tablet Monitor Pen").unwrap()
		.event(uinputController(uinputDigi(uinputTouch))).unwrap()
		.event(uinputController(uinputDigi(uinputPen))).unwrap()
		.event(uinputController(uinputDigi(uinputStylus))).unwrap()
		.event(uinputController(uinputDigi(uinputStylus2))).unwrap()
		.event(uinputPositionX).unwrap()
		.event(uinputPositionY).unwrap()
		.event(uinputPressure).unwrap()
		.event(uinputTiltX).unwrap()
		.event(uinputTiltY).unwrap()
		.create().unwrap();

	/*system_device.send(uinputPositionX, 500);
	system_device.send(uinputPositionY, 500);
	system_device.synchronize().unwrap();*/

	let mut buffer: [u8;12] = [0;12];

	loop {
		let result = handler.read_bulk(endpoint, &mut buffer, std::time::Duration::new(5,0));

		if result.is_err() {
			continue;
		}
		
		//println!("{:?}", buffer);

		let pen = parse_usb_buffer_pen(buffer);
		let position = parse_pen_position(buffer);
		let pressure = parse_pen_pressure(buffer);
		let tilt = parse_pen_tilt(buffer);

		println!("Parsed pen: X:{:5} Y:{:5} Pressure:{:4} Tilt X:{:4} Tilt Y:{:4} Hover:{:5} Touch:{:5} Buttonbar:{:5} Scrollbar:{:5}", 
			position.0, position.1, pressure, tilt.0, tilt.1, pen.0, pen.1, pen.2, pen.3);
	}
}

fn parse_pen_tilt(data: [u8;12]) -> (i8, i8) {

	//Unsafe code because the values must be signed numbers (seems that they can be negative).
	let tilt_x = unsafe { std::mem::transmute::<u8, i8>(data[10]) };
	let tilt_y = unsafe { std::mem::transmute::<u8, i8>(data[11]) };

	let tilt_y = - tilt_y;

	//println!("{:6} {:6}", tilt_x, tilt_y);
	
	(tilt_x, tilt_y)
}

fn parse_pen_position(data: [u8;12]) -> (u32, u32) {

	let x = (u32::from(data[8]) << 16) + (u32::from(data[3]) << 8) + (u32::from(data[2]));
	let y = (u32::from(data[9]) << 16) + (u32::from(data[5]) << 8) + (u32::from(data[4]));

	//println!("{:#32b} {:#32b}", x, y);
	
	(x, y)
}

fn parse_pen_pressure(data: [u8;12]) -> u16 {

	//println!("{:#16b}", (u16::from(data[7]) << 8) + (u16::from(data[6])));
	
	(u16::from(data[7]) << 8) + (u16::from(data[6]))
}

fn parse_usb_buffer_pen(data: [u8;12]) -> (bool, bool, bool, bool) {
	//println!("Pen event: {:#b}", data[1]);

	let pen_buttons = data[1];

	let is_hover 		= (pen_buttons & 0b1000_0000) == 0b1000_0000;
	let is_touch 		= (pen_buttons & 0b0000_0001) == 0b0000_0001;
	let is_buttonbar 	= (pen_buttons & 0b0000_0010) == 0b0000_0010;
	let is_scrollbar 	= (pen_buttons & 0b0000_0100) == 0b0000_0100;

	(is_hover, is_touch, is_buttonbar, is_scrollbar)
}

fn detach_kernel(config_descriptor: &rusb::ConfigDescriptor, handler: &mut rusb::DeviceHandle) -> Vec<u8> {

	println!("\nFinding interfaces...");

	let mut available_endpoints: Vec<u8> = vec!();

	for interface in config_descriptor.interfaces() {
		let interface_number = interface.number();

		available_endpoints.push(get_a_endpoint(&interface));

		println!("\tFound interface: {:?}", interface.number());

		let is_kernel_active = handler.kernel_driver_active(interface.number())
		.unwrap_or_else(|_| { panic!(format!("Error checking if kernel driver is active interface: {}", interface_number).to_owned()) });

		if is_kernel_active { 
			handler.detach_kernel_driver(interface.number())
			.unwrap_or_else(|_| { panic!(format!("Error detaching kernel driver: {}", interface_number).to_owned()) });
		}

		handler.claim_interface(interface.number())
		.unwrap_or_else(|_| { panic!(format!("Error claiming interface: {}", interface_number).to_owned()) });

		println!("\tClaimed interface {}", interface_number);
	}

	println!();

	available_endpoints
}

fn get_a_endpoint(interface: &rusb::Interface) -> u8 {
	let interface_descriptors: Vec<rusb::InterfaceDescriptor> = interface.descriptors().collect();

	let interface_descriptor = &interface_descriptors[0];

	let endpoint_descriptors: Vec<rusb::EndpointDescriptor> = interface_descriptor.endpoint_descriptors().collect();

	let endpoint_descriptor = &endpoint_descriptors[0];

	endpoint_descriptor.address()
}
