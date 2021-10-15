use nix::unistd::Pid;
use std::{path::PathBuf, str::FromStr};

use crate::state;

pub fn kill_container(project_dir: PathBuf, id: Option<&str>, signal_str: Option<&str>, all: bool) {
	let container_state = state::get_container_state(project_dir, id.unwrap());
	if container_state.status != "created" && container_state.status != "running" {
		panic!("Cannot send signals to non-running containers!")
	}

	if all {
		unimplemented!("Sending signals to all container processes is currently unimplemented!");
	}

	let pid = container_state.pid.unwrap();
	let signal = if !signal_str.unwrap().starts_with("SIG") {
		"SIG".to_owned() + signal_str.unwrap()
	} else {
		signal_str.unwrap().to_owned()
	};
	nix::sys::signal::kill(
		Pid::from_raw(pid),
		nix::sys::signal::Signal::from_str(signal.as_str())
			.expect(format!("Could not parse signal string {}", signal_str.unwrap()).as_str()),
	)
	.expect(
		format!(
			"Could not send signal {} to container process ID  {}!",
			signal_str.unwrap(),
			pid
		)
		.as_str(),
	);
}
