use crate::container::OCIContainer;
use std::fs::{self, File};
use std::io::{Read, Write};

pub fn start_container(id: Option<&str>) {
	let mut path = crate::get_project_dir();
	path.push(id.unwrap());

	if let Ok(mut file) = fs::OpenOptions::new()
		.read(true)
		.write(false)
		.open(path.join("container.json"))
	{
		let mut contents = String::new();
		file.read_to_string(&mut contents)
			.expect("Unable to read container specification");

		// Do we have a valid container?
		if let Ok(container) = serde_json::from_str::<OCIContainer>(&contents) {
			debug!("Bundle at {}", container.bundle());
			debug!(
				"Start container with uid {}, gid {}",
				container.spec().process.as_ref().unwrap().user.uid,
				container.spec().process.as_ref().unwrap().user.gid
			);

			debug!("Open exec fifo to start container!");
			let mut fifo = File::open(path.join("exec.fifo")).expect("Could not open exec fifo!");
			let mut buffer = [1u8];
			fifo.read_exact(&mut buffer)
				.expect("Could not read from exec fifo!");

			if buffer[0] == 0 {
				info!("Container started successfully!");
			} else {
				panic!(
					"Invalid value read from fifo. Read byte was {:x}",
					buffer[0]
				);
			}
		}
	} else {
		println!("Container `{}` doesn't exists", id.unwrap());
	}
}
