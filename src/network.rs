use futures::TryStreamExt;
use rtnetlink::Error;
use std::{net::Ipv4Addr, process::Stdio};

pub struct HermitNetworkConfig {
	pub ip: Ipv4Addr,
	pub gateway: Ipv4Addr,
	pub mask: Ipv4Addr,
	pub mac: String
}

pub async fn set_lo_up() -> Result<(), Error> {
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

fn run_command(command: &str, args: Vec<&str>) -> String {
	info!("Running command {} with args {}", command, args.join(" "));
	let ret = std::process::Command::new(command)
		.args(&args)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.expect("Unable to spawn ip script process")
		.wait_with_output()
		.unwrap();
	if !ret.status.success() {
		panic!(
			"Command {} with args {} failed! Exit code {}, stderr: {}",
			command,
			args.join(" "),
			ret.status,
			String::from_utf8(ret.stderr).unwrap()
		);
	}
	let output = String::from_utf8(ret.stdout).unwrap();
	debug!("Command {} produced output {}", command, output);
	output
}

/**
 This function is in large parts inspired by the runnc code for Nabla Containers
 https://github.com/nabla-containers/runnc/blob/46ededdd75a03cecf05936a1a45d5d0096a2b117/nabla-lib/network/network_linux.go
*/
pub async fn create_tap() -> Result<HermitNetworkConfig, Error> {
	//FIXME: This is extremely ugly
	let inet_str_output = run_command(
		"/bin/sh",
		vec![
			"-c",
			r#"echo `ip addr show dev eth0  | grep "inet\ " | awk '{print $2}'`"#,
		],
	);
	let inet_str = inet_str_output.trim_end_matches("\n");
	let mut inet_str_split = inet_str.split("/");

	let mac_str_output = run_command(
		"/bin/sh",
		vec![
			"-c",
			r#"echo `ip addr show dev eth0  | grep "link/ether\ " | awk '{print $2}'`"#,
		],
	);
	let mac_str = mac_str_output.trim_end_matches("\n");

	let ip_addr = inet_str_split.next().unwrap();
	let cidr = inet_str_split.next().unwrap();

	let gateway_output = run_command(
		"/bin/sh",
		vec![
			"-c",
			r#"echo `ip route | grep ^default | awk '{print $3}'`"#,
		],
	);
	let gateway = gateway_output.trim_end_matches("\n");

	let _ = run_command("ip", vec!["tuntap", "add", "tap100", "mode", "tap"]);
	let _ = run_command("ip", vec!["link", "set", "dev", "tap100", "up"]);
	let _ = run_command("ip", vec!["addr", "delete", inet_str, "dev", "eth0"]);
	let _ = run_command("ip", vec!["link", "set", "eth0", "address", "aa:aa:aa:aa:bb:cc"]); //Random MAC taken from the Nabla code
	let _ = run_command("ip", vec!["link", "add", "name", "br0", "type", "bridge"]);
	let _ = run_command("ip", vec!["link", "set", "eth0", "master", "br0"]);
	let _ = run_command("ip", vec!["link", "set", "tap100", "master", "br0"]);
	let _ = run_command("ip", vec!["link", "set", "br0", "up"]);

	info!(
		"Executed ip commands: IP={},MASK={},GW={},MAC={}",
		ip_addr, cidr, gateway, mac_str
	);

	let cidr_int: u32 = cidr.parse().unwrap();
	let mask: Ipv4Addr = Ipv4Addr::from(0xffffffffu32 << cidr_int);
	Ok(HermitNetworkConfig {
		ip: ip_addr.parse().unwrap(),
		gateway: gateway.parse().unwrap(),
		mask,
		mac: mac_str.to_string()
	})
}
