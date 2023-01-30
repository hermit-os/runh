use crate::container::OCIContainer;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;

pub fn start_container(mut project_dir: PathBuf, id: &str) {
	project_dir.push(id);

	if let Ok(mut file) = fs::OpenOptions::new()
		.read(true)
		.write(false)
		.open(project_dir.join("container.json"))
	{
		let mut contents = String::new();
		file.read_to_string(&mut contents)
			.expect("Unable to read container specification");

		// Do we have a valid container?
		if let Ok(container) = serde_json::from_str::<OCIContainer>(&contents) {
			debug!("Bundle at {}", container.bundle());
			debug!(
				"Start container with uid {}, gid {}",
				container.spec().process().as_ref().unwrap().user().uid(),
				container.spec().process().as_ref().unwrap().user().gid()
			);

			debug!("Open exec fifo to start container!");
			let mut fifo =
				File::open(project_dir.join("exec.fifo")).expect("Could not open exec fifo!");
			let mut buffer = [1u8];
			fifo.read_exact(&mut buffer)
				.expect("Could not read from exec fifo!");
			drop(fifo);

			if buffer[0] == 0 {
				info!("Container started successfully! Deleting exec fifo!");
				std::fs::remove_file(project_dir.join("exec.fifo"))
					.expect("Could not delete exec fifo!");
			} else {
				panic!(
					"Invalid value read from fifo. Read byte was {:x}",
					buffer[0]
				);
			}
		}
	} else {
		println!("Container `{id}` doesn't exists");
	}
}
