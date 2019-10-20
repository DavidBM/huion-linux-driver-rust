use std::convert::TryInto;

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

	let virtual_input_device = create_virtual_input_device(&device, &device_desc);

	let mut buffer: [u8;12] = [0;12];

	loop {
		let result = handler.read_bulk(endpoint, &mut buffer, std::time::Duration::new(5,0));

		if result.is_err() {
			println!("{:?}", result);
			continue;
		}

		let pen = parse_usb_buffer_pen(buffer);
		let position = parse_pen_position(buffer);
		let pressure = parse_pen_pressure(buffer);
		let tilt = parse_pen_tilt(buffer);

		send_events_to_virtual_device(&virtual_input_device, pen, position, pressure, tilt);

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

fn create_virtual_input_device(_usb_device: &rusb::Device, device_desc: &rusb::DeviceDescriptor) -> evdev_rs::uinput::UInputDevice {
	use evdev_rs::enums::EventCode;
	use evdev_rs::enums::EV_KEY;
	use evdev_rs::enums::EV_ABS;
	let device = evdev_rs::Device::new().unwrap();

	let device_version = device_desc.device_version();
	let device_version = (u16::from(device_version.major()) << 8) + (u16::from(device_version.minor()) << 4) + u16::from(device_version.sub_minor());

	let now = std::time::SystemTime::now();
	let since_epoch = now.duration_since(std::time::UNIX_EPOCH).unwrap();

	device.set_name(&*format!("{} {:?}", "Tablet Monitor Touch Display", since_epoch.as_millis()));
	device.set_phys("HDMI1");
	device.set_bustype(0x3);
	device.set_vendor_id(device_desc.vendor_id().try_into().unwrap());
	device.set_product_id(device_desc.product_id().try_into().unwrap());
	device.set_product_id(device_version.try_into().unwrap());

	device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_TOUCH), None).unwrap();
	device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_TOOL_PEN), None).unwrap();
	device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_STYLUS), None).unwrap();
	device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_STYLUS2), None).unwrap();

	device.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_X), Some(&create_absinfo(86967, 3, 195))).unwrap();
	device.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_Y), Some(&create_absinfo(47746, 6, 201))).unwrap();
	device.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_PRESSURE), Some(&create_absinfo(8191, 0, 0))).unwrap();
	device.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_TILT_X), Some(&create_absinfo(127, -127, 0))).unwrap();
	device.enable_event_code(&EventCode::EV_ABS(EV_ABS::ABS_TILT_Y), Some(&create_absinfo(127, -127, 0))).unwrap();

	evdev_rs::UInputDevice::create_from_device(&device).unwrap()
}

fn create_absinfo(maximum: i32, minimum: i32, resolution: i32) -> evdev_rs::AbsInfo {
	evdev_rs::AbsInfo {
		value: 0,
		minimum,
		maximum,
		fuzz: 0,
		flat: 0,
		resolution,
	}
}

fn send_events_to_virtual_device(
	device: &evdev_rs::uinput::UInputDevice,
	pen: (bool, bool, bool, bool), 
	position: (u32, u32), 
	pressure: u16, 
	tilts: (i8, i8)
) {

	use evdev_rs::InputEvent;
	use evdev_rs::TimeVal;
	use evdev_rs::enums::EventCode;
	use evdev_rs::enums::EV_KEY;
	use evdev_rs::enums::EV_ABS;
	use evdev_rs::enums::EV_SYN;

	let now = std::time::SystemTime::now();
	let since_epoch = now.duration_since(std::time::UNIX_EPOCH).unwrap();

	let timeval = &TimeVal {
		tv_sec: since_epoch.as_secs() as i64,
		tv_usec: i64::from(since_epoch.subsec_micros()),
	};

	let mut events: Vec<InputEvent> = vec!();

	events.push(InputEvent::new(&timeval, &EventCode::EV_KEY(EV_KEY::BTN_TOUCH), if pen.1 { 1 } else { 0 }));
	events.push(InputEvent::new(&timeval, &EventCode::EV_KEY(EV_KEY::BTN_STYLUS), if pen.2 { 1 } else { 0 }));
	events.push(InputEvent::new(&timeval, &EventCode::EV_KEY(EV_KEY::BTN_STYLUS2), if pen.3 { 1 } else { 0 }));
	events.push(InputEvent::new(&timeval, &EventCode::EV_KEY(EV_KEY::BTN_STYLUS2), if pen.3 { 1 } else { 0 }));

	events.push(InputEvent::new(&timeval, &EventCode::EV_ABS(EV_ABS::ABS_X), position.0.try_into().unwrap()));
	events.push(InputEvent::new(&timeval, &EventCode::EV_ABS(EV_ABS::ABS_Y), position.1.try_into().unwrap()));
	events.push(InputEvent::new(&timeval, &EventCode::EV_ABS(EV_ABS::ABS_PRESSURE), pressure.try_into().unwrap()));
	events.push(InputEvent::new(&timeval, &EventCode::EV_ABS(EV_ABS::ABS_TILT_X), tilts.0.into()));
	events.push(InputEvent::new(&timeval, &EventCode::EV_ABS(EV_ABS::ABS_TILT_Y), tilts.1.into()));

	events.push(InputEvent::new(&timeval, &EventCode::EV_SYN(EV_SYN::SYN_REPORT), 0));

	for event in events {
		device.write_event(&event).unwrap();
	}
}
