use nix::unistd::Pid;
use std::{convert::TryFrom, path::PathBuf, str::FromStr};

use crate::state;

pub fn kill_container(project_dir: PathBuf, id: &str, sig: &str, all: bool) {
	let container_state = state::get_container_state(project_dir, id)
		.unwrap_or_else(|| panic!("Could not query state for container {}", id));
	if container_state.status != "created" && container_state.status != "running" {
		panic!("Cannot send signals to non-running containers!")
	}

	if all {
		unimplemented!("Sending signals to all container processes is currently unimplemented!");
	}

	let pid = container_state.pid.unwrap();
	let signal = if let Ok(sig_nr) = sig.parse::<i32>() {
		nix::sys::signal::Signal::try_from(sig_nr)
			.unwrap_or_else(|_| panic!("Could not parse signal number {}", sig))
	} else {
		let signal_str = if !sig.starts_with("SIG") {
			format!("SIG{sig}")
		} else {
			sig.to_owned()
		};
		nix::sys::signal::Signal::from_str(signal_str.as_str())
			.unwrap_or_else(|_| panic!("Could not parse signal string {}", sig))
	};

	nix::sys::signal::kill(Pid::from_raw(pid), signal).unwrap_or_else(|_| {
		panic!(
			"Could not send signal {} to container process ID  {}!",
			sig, pid
		)
	});
}
