use futures::TryStreamExt;
use netlink_packet_core::ErrorMessage;
use netlink_packet_route::address::AddressAttribute;
use netlink_packet_route::link::LinkAttribute;
use netlink_packet_route::route::{RouteAddress, RouteAttribute};
use nix::sys::stat::SFlag;
use rtnetlink::Error::NetlinkError;
use std::net::IpAddr;
use std::num::NonZeroI32;
use std::path::PathBuf;
use std::{error::Error, fmt, net::Ipv4Addr};

// FIXME: https://github.com/rust-netlink/netlink-packet-route/issues/88
const MACVLAN_MODE_PASSTHRU: u32 = 8;

#[derive(Debug)]
struct VirtioNetworkError {
	details: String,
}

#[derive(Debug)]
pub struct VirtioNetworkConfig {
	pub ip: Ipv4Addr,
	pub gateway: Ipv4Addr,
	pub mask: Ipv4Addr,
	pub mac: String,
	pub macvtap_index: u32,
}

impl From<String> for VirtioNetworkError {
	fn from(msg: String) -> Self {
		VirtioNetworkError { details: msg }
	}
}

impl fmt::Display for VirtioNetworkError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.details)
	}
}

impl Error for VirtioNetworkError {
	fn description(&self) -> &str {
		&self.details
	}
}

pub async fn set_lo_up() -> Result<(), rtnetlink::Error> {
	let (connection, handle, _) = rtnetlink::new_connection().unwrap();
	tokio::spawn(connection);
	let mut links = handle.link().get().match_name("lo".to_string()).execute();
	if let Some(link) = links.try_next().await? {
		handle.link().set(link.header.index).up().execute().await?
	} else {
		panic!("Link lo not found!");
	}

	Ok(())
}

/**
 This function is in large parts inspired by the runnc code for Nabla Containers
 https://github.com/nabla-containers/runnc/blob/46ededdd75a03cecf05936a1a45d5d0096a2b117/nabla-lib/network/network_linux.go
*/
pub async fn create_tap() -> Result<VirtioNetworkConfig, Box<dyn std::error::Error>> {
	let (connection, handle, _) = rtnetlink::new_connection()?;
	tokio::spawn(connection);

	// Check for an existing tap device
	let mut tap_link_req = handle
		.link()
		.get()
		.match_name("macvtap0".to_string())
		.execute();

	let do_init = match tap_link_req.try_next().await {
		Ok(Some(_)) => {
			warn!("Tap device already exists in current network namespace. Trying to read configuration from eth0 / macvtap0 device...");
			false
		}
		Ok(None) => {
			warn!("Tap device exists in namespace but cannot be read. Trying to re-do setup...");
			true
		}
		Err(NetlinkError(ErrorMessage { code, .. })) if code == NonZeroI32::new(-libc::ENODEV) => {
			// This is the expected case that is triggered when the tap device does not exist in the current namespace
			true
		}
		Err(err) => {
			return Err(Box::new(VirtioNetworkError::from(format!(
				"Macvtap0 interface detection failed: {err}"
			))));
		}
	};

	// Get link info for eth0 device
	let link_info = handle
		.link()
		.get()
		.match_name(String::from("eth0"))
		.execute()
		.try_next()
		.await?
		.expect("Could not read link info for interface eth0!");

	// Extract device index from link info
	let eth0_device_index = link_info.header.index;

	//Setup network parameters
	let mut mac_address: Option<String> = None;
	let mut ip_address: Option<Ipv4Addr> = None;
	let mut prefix_length: Option<u8> = None;
	let mut gateway_address: Option<Ipv4Addr> = None;

	// Get address info for eth0 device
	let device_addr_msg = handle
		.address()
		.get()
		.set_link_index_filter(eth0_device_index)
		.execute()
		.try_next()
		.await?
		.expect("Could not read address info for interface eth0!");

	// Extract IP address from address info
	for address_attribute in device_addr_msg.attributes.into_iter() {
		if let AddressAttribute::Address(addr) = address_attribute {
			match addr {
				IpAddr::V4(addr) => ip_address = Some(addr),
				IpAddr::V6(_) => panic!(),
			}
			prefix_length = Some(device_addr_msg.header.prefix_len);
		}
	}

	// Get route info and extract gateway address
	let mut route_get_req = handle.route().get(rtnetlink::IpVersion::V4).execute();
	while let Some(route_msg) = route_get_req.try_next().await? {
		for route_attribute in route_msg.attributes.into_iter() {
			if let RouteAttribute::Gateway(addr) = route_attribute {
				match addr {
					RouteAddress::Inet(addr) => gateway_address = Some(addr),
					_ => panic!(),
				}
				break;
			}
		}
	}

	if do_init {
		// Create macvtap0 interface
		handle
			.link()
			.add()
			.macvtap("macvtap0".into(), eth0_device_index, MACVLAN_MODE_PASSTHRU)
			.execute()
			.await?;
	}

	// Determine index of newly created macvtap
	let macvtap_link_info = handle
		.link()
		.get()
		.match_name("macvtap0".into())
		.execute()
		.try_next()
		.await?
		.expect("Could not read link info for interface macvtap0!");

	let macvtap_index = macvtap_link_info.header.index;

	// Extract mac from macvtap
	for link_attribute in macvtap_link_info.attributes.into_iter() {
		if let LinkAttribute::Address(addr) = link_attribute {
			if addr.len() != 6 {
				return Err(Box::new(VirtioNetworkError::from(format!(
					"Received invalid MAC address {addr:?} for macvtap device!"
				))));
			}
			mac_address = Some(format!(
				"{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
				addr[0], addr[1], addr[2], addr[3], addr[4], addr[5]
			));
			debug!(
				"Found macvtap mac address: {}",
				mac_address.as_ref().unwrap()
			);
			break;
		}
	}

	// Read tap device numbers associated with macvtap
	let tap_dev_file_path = PathBuf::from("/sys/class/net/macvtap0/macvtap")
		.join(format!("tap{macvtap_index}"))
		.join("dev");
	let dev_file_string = std::fs::read_to_string(&tap_dev_file_path)
		.unwrap_or_else(|_| panic!("Could not open sysfs entry at {:?}", &tap_dev_file_path));

	let mut major_minor_split = dev_file_string.split(':');

	let major: u64 = major_minor_split.next().unwrap().trim().parse().unwrap();
	let minor: u64 = major_minor_split.next().unwrap().trim().parse().unwrap();

	// Create tap device in container
	let device = nix::sys::stat::makedev(major, minor);
	nix::sys::stat::mknod(
		&PathBuf::from(format!("/dev/tap{macvtap_index}")),
		SFlag::S_IFCHR,
		nix::sys::stat::Mode::from_bits(0o600u32).unwrap(),
		device,
	)
	.expect("Could not create tap device corresponding to macvtap0!");

	// Assume all network parameters have been set
	let ip_address =
		ip_address.expect("IP address could not be determined during networking setup!");
	let prefix_length =
		prefix_length.expect("IP prefix length could not be determined during networking setup!");
	let gateway_address =
		gateway_address.expect("Gateway address could not be determined during networking setup!");
	let mac_address =
		mac_address.expect("MAC address could not be determined during networking setup!");

	info!(
		"Found / created network setup: IP={},MASK={},GW={},MAC={}",
		ip_address, prefix_length, gateway_address, &mac_address
	);

	let mask: Ipv4Addr = Ipv4Addr::from(0xffffffffu32 << prefix_length);
	Ok(VirtioNetworkConfig {
		ip: ip_address,
		gateway: gateway_address,
		mask,
		mac: mac_address,
		macvtap_index,
	})
}
