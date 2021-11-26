use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, net::Ipv4Addr, process::Stdio};

#[derive(Debug)]
struct HermitNetworkError {
	details: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HermitNetworkConfig {
	pub ip: Ipv4Addr,
	pub gateway: Ipv4Addr,
	pub mask: Ipv4Addr,
	pub mac: String,
	pub network_namespace: Option<String>,
	#[serde(skip)]
	pub did_init: bool,
}

impl From<String> for HermitNetworkError {
	fn from(msg: String) -> Self {
		HermitNetworkError { details: msg }
	}
}

impl fmt::Display for HermitNetworkError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.details)
	}
}

impl Error for HermitNetworkError {
	fn description(&self) -> &str {
		&self.details
	}
}

pub async fn set_lo_up() -> Result<(), rtnetlink::Error> {
	let (connection, handle, _) = rtnetlink::new_connection().unwrap();
	tokio::spawn(connection);
	let mut links = handle
		.link()
		.get()
		.set_name_filter("lo".to_string())
		.execute();
	if let Some(link) = links.try_next().await? {
		handle.link().set(link.header.index).up().execute().await?
	} else {
		panic!("Link lo not found!");
	}
	Ok(())
}

pub fn undo_tap_creation(config: &HermitNetworkConfig) -> Result<(), Box<dyn std::error::Error>> {
	let _ = run_command("ip", vec!["tuntap", "del", "tap100", "mode", "tap"]);
	let _ = run_command("ip", vec!["link", "delete", "br0"]);
	let _ = run_command("ip", vec!["link", "delete", "dummy0"]);
	let _ = run_command(
		"ip",
		vec!["link", "set", "eth0", "address", config.mac.as_str()],
	);
	let _ = run_command(
		"ip",
		vec![
			"addr",
			"add",
			format!(
				"{}/{}",
				config.ip.to_string(),
				u32::from(config.mask).trailing_zeros()
			)
			.as_str(),
			"dev",
			"eth0",
		],
	);
	Ok(())
}

fn run_command(command: &str, args: Vec<&str>) -> Result<String, Box<dyn std::error::Error>> {
	info!("Running command {} with args {}", command, args.join(" "));
	let ret = std::process::Command::new(command)
		.args(&args)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()?
		.wait_with_output()?;
	let stderr = String::from_utf8(ret.stderr)?;
	if !ret.status.success() {
		return Err(Box::new(HermitNetworkError::from(format!(
			"Command {} with args {} failed! Exit code {}, stderr: {}",
			command,
			args.join(" "),
			ret.status,
			stderr
		))));
	}
	let output = String::from_utf8(ret.stdout)?;
	debug!("Command {} produced output {}", command, output);
	Ok(output)
}

/**
 This function is in large parts inspired by the runnc code for Nabla Containers
 https://github.com/nabla-containers/runnc/blob/46ededdd75a03cecf05936a1a45d5d0096a2b117/nabla-lib/network/network_linux.go
*/
pub async fn create_tap(
	network_namespace: Option<String>,
) -> Result<HermitNetworkConfig, Box<dyn std::error::Error>> {
	//FIXME: This is extremely ugly
	let mut do_init = false;

	let _ = run_command(
		//Debugging
		"ip",
		vec!["addr"],
	);

	let (connection, handle, _) = rtnetlink::new_connection()?;
	tokio::spawn(connection);
	let mut links = handle
		.link()
		.get()
		.set_name_filter("tap100".to_string())
		.execute();
	let read_interface = if links.try_next().await?.is_none() {
		do_init = true;
		"eth0"
	} else {
		warn!("Tap device already exists in current network namespace. Trying to read configuration from dummy device...");
		"dummy0"
	};

	let inet_str_output = run_command(
		"/bin/sh",
		vec![
			"-c",
			if do_init {
				"echo `ip addr show dev eth0  | grep \"inet\\ \" | awk '{print $2}'`"
			} else {
				"echo `ip addr show dev dummy0  | grep \"dummy0:ip\" | awk '{print $2}'`"
			}
		],
	)?;
	let inet_str = inet_str_output.trim_end_matches("\n");

	if inet_str.is_empty() {
		//warn!("Could not perform network setup! eth0 interface does not exist!");
		return Err(Box::new(HermitNetworkError::from(format!(
			"Could not perform network setup! {} interface does not exist!",
			read_interface
		))));
	}

	let mut inet_str_split = inet_str.split("/");

	let mac_str_output = run_command(
		"/bin/sh",
		vec![
			"-c",
			format!(
				"echo `ip addr show dev {}  | grep \"link/ether\\ \" | awk '{{print $2}}'`",
				read_interface
			)
			.as_str(),
		],
	)?;
	let mac_str = mac_str_output.trim_end_matches("\n");

	let ip_addr = inet_str_split.next().unwrap();
	let cidr = inet_str_split.next().unwrap();

	let gateway_output = run_command(
		"/bin/sh",
		vec![
			"-c",
			if do_init {
				r#"echo `ip route | grep ^default | awk '{print $3}'`"#
			} else {
				"echo `ip addr show dev dummy0  | grep \"dummy0:gw\" | awk '{print $2}' | awk -F '/' '{print $1}'`"
			}
		],
	)?;
	let gateway_masked = gateway_output.trim_end_matches("\n");

	if do_init {
		let _ = run_command("ip", vec!["tuntap", "add", "tap100", "mode", "tap"]);
		let _ = run_command("ip", vec!["link", "set", "dev", "tap100", "up"]);
		let _ = run_command("ip", vec!["addr", "delete", inet_str, "dev", "eth0"]);
		let _ = run_command(
			"ip",
			vec!["link", "set", "eth0", "address", "aa:aa:aa:aa:bb:cc"],
		); //Random MAC taken from the Nabla code
		let _ = run_command("ip", vec!["link", "add", "name", "br0", "type", "bridge"]);
		let _ = run_command("ip", vec!["link", "set", "eth0", "master", "br0"]);
		let _ = run_command("ip", vec!["link", "set", "tap100", "master", "br0"]);
		let _ = run_command("ip", vec!["link", "set", "br0", "up"]);
		// Set up dummy device
		// This is even uglier: We need to save the addresses we obtained from eth0 somewhere in the network namespace so that
		// future unikernel instances in the same namespace can access it. This becomes relevant for restarting Kubernetes Pods
		// because Kubernetes / CRI-O just does spawns a new container on restart in the Pod network namespace without deleting the old one first.
		let _ = run_command("ip", vec!["link", "add", "dummy0", "type", "dummy"]);
		let _ = run_command("ip", vec!["addr", "add", inet_str, "label", "dummy0:ip", "dev", "dummy0"]);
		let _ = run_command("ip", vec!["addr", "add", gateway, "label", "dummy0:gw", "dev", "dummy0"]);
		let _ = run_command("ip", vec!["link", "set", "dummy0", "address", mac_str]);
	}

	info!(
		"Found / created network setup: IP={},MASK={},GW={},MAC={}",
		ip_addr, cidr, gateway, mac_str
	);

	let cidr_int: u32 = cidr.parse().unwrap();
	let mask: Ipv4Addr = Ipv4Addr::from(0xffffffffu32 << cidr_int);
	Ok(HermitNetworkConfig {
		ip: ip_addr.parse().unwrap(),
		gateway: gateway.parse().unwrap(),
		mask,
		mac: mac_str.to_string(),
		network_namespace,
		did_init: do_init,
	})
}
