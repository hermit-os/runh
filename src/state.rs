use crate::{consts, container::OCIContainer};
use serde::*;
use std::{collections::HashMap, fs::OpenOptions, io::BufReader, path::PathBuf};

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
	#[serde(rename = "ociVersion")]
	pub version: String,
	pub id: String,
	pub status: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pid: Option<i32>,
	pub bundle: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub annotations: Option<HashMap<String, String>>,
}

pub fn get_container_state(project_dir: PathBuf, id: &str) -> Option<State> {
	let container_dir = project_dir.join(id);
	if !container_dir.is_dir() {
		warn!("Could not query state. Container {id} does not exist in project dir!");
		return None;
	}

	let exec_fifo = container_dir.join("exec.fifo");

	let bundle = String::from(
		container_dir
			.join("bundle")
			.read_link()
			.expect("Could not query state. Bundle link could not be read!")
			.to_str()
			.unwrap(),
	);

	let state_file = container_dir.join("created");
	let pid: Option<i32> = if state_file.exists() {
		Some(
			std::fs::read_to_string(state_file)
				.expect("Could not query state. State file could not be read!")
				.parse()
				.expect("Could not query state. pid could not be parsed from state file!"),
		)
	} else {
		None
	};

	let container_file_path = container_dir.join("container.json");
	let container_file = OpenOptions::new()
		.read(true)
		.open(container_file_path)
		.expect("Could not query state. Container file could not be opened!");
	let container: OCIContainer = serde_json::from_reader(BufReader::new(container_file))
		.expect("Could not query state. Container file could not be parsed!");

	Some(State {
		version: String::from(consts::OCI_STATE_VERSION),
		id: id.to_string(),
		status: String::from(if let Some(pid_int) = pid {
			if let Ok(process) = procfs::process::Process::new(pid_int) {
				let process_state = process
					.stat()
					.expect("Could not query state. Process stat could not be read!")
					.state()
					.expect("Could not query state. Process state could not be read!");
				match process_state {
					procfs::process::ProcState::Zombie => "stopped",
					procfs::process::ProcState::Dead => "stopped",
					_ => {
						if exec_fifo.exists() {
							"created"
						} else {
							"running"
						}
					}
				}
			} else {
				"stopped"
			}
		} else {
			"creating"
		}),
		pid,
		bundle,
		annotations: container.spec().annotations().clone(),
	})
}

pub fn print_container_state(project_dir: PathBuf, id: &str) {
	println!(
		"{}",
		serde_json::to_string(
			&get_container_state(project_dir, id).unwrap_or_else(|| panic!(
				"Could not query state. Container {} does not exist!",
				id
			))
		)
		.expect("Could not query state. State could not be serialized!")
	);
}
