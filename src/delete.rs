use crate::container::OCIContainer;
use std::fs;
use std::io::Read;

pub fn delete_container(id: Option<&str>) {
	let mut delete_file = false;
	let mut path = crate::get_project_dir();
	path.push(id.unwrap());
	// path to the container specification
	let container_dir = path.clone();
	path.push("container.json");

	if let Ok(mut file) = fs::OpenOptions::new().read(true).write(false).open(path) {
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
