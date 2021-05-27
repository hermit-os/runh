use std::fs::OpenOptions;
use std::io::prelude::*;

use crate::container::OCIContainer;

pub fn create_container(id: Option<&str>, bundle: Option<&str>) {
	let container = OCIContainer::new(bundle.unwrap().to_string(), id.unwrap().to_string());
	let mut path = crate::get_project_dir();

	let _ = std::fs::create_dir(path.clone());

	path.push(id.unwrap());
	std::fs::create_dir(path.clone()).expect("Unable to create container directory");

	// write container to disk
	path.push("container.json");
	let mut file = OpenOptions::new()
		.read(true)
		.write(true)
		.create_new(true)
		.open(path)
		.expect("Unable to create container");
	file.write_all(serde_json::to_string(&container).unwrap().as_bytes())
		.unwrap();
}
