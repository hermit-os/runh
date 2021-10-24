use std::process::Stdio;

pub fn setup_network() {
	//TODO: Replace call to ip binary with library calls
	let ret = std::process::Command::new("ip")
		.arg("link")
		.arg("set")
		.arg("lo")
		.arg("up")
		.stderr(Stdio::piped())
		.spawn()
		.expect("Unable to spawn ip process")
		.wait_with_output()
		.unwrap();
	if !ret.status.success() {
		panic!(
			"ip link set lo up returned exit status {}. Stderr: {}",
			ret.status,
			String::from_utf8(ret.stderr).unwrap()
		);
	}
}

// async fn setup_network_async(connection: Connection<RtnlMessage>, handle: Handle) {
//     // //tokio::spawn(connection);

//     // let mut links = handle.link().get().set_name_filter(String::from("lo")).execute();
// 	// if let Some(link) = links.try_next().await.unwrap() {
// 	// 	handle.link().set(link.header.index).down().execute().await
//     //         .expect("Could not set link lo to up!");
// 	// } else {
// 	// 	panic!("Could not find loopback link!");
// 	// }
// }
