use crate::container::OCIContainer;
use crate::kill;
use crate::state;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

pub fn delete_container(mut project_dir: PathBuf, id: Option<&str>, force: bool) {
	let container_state = state::get_container_state(project_dir.clone(), id.unwrap());
	if container_state.status != "stopped" {
		if !force {
			panic!("Tried to delete a container that is not stopped!");
		} else {
			warn!("Container is still running. Force-deleting...");
			kill::kill_container(project_dir.clone(), id, Some("SIGKILL"), false);
		}
	}

	let mut delete_file = false;
	project_dir.push(id.unwrap());
	// path to the container specification
	let container_dir = project_dir.clone();
	project_dir.push("container.json");

	if let Ok(mut file) = fs::OpenOptions::new()
		.read(true)
		.write(false)
		.open(project_dir)
	{
		let mut contents = String::new();
		file.read_to_string(&mut contents)
			.expect("Unable to read container specification");

		if let Ok(_container) = serde_json::from_str::<OCIContainer>(&contents) {
			delete_file = true;
		}
	} else {
		println!("Container `{}` doesn't exists", id.unwrap());
	}

	if delete_file {
		// delete all temporary files
		fs::remove_dir_all(container_dir).expect("Unable to delete container");
	}
}
