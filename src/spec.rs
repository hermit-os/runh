use oci_spec::runtime;
use std::fs;
use std::path;

pub fn create_spec(bundle: Option<&str>, args: Vec<String>) {
	let dir = fs::canonicalize(path::PathBuf::from(bundle.unwrap()))
		.expect("Unable to determine absolute bundle path");
	let mut config_file = dir.clone();
	config_file.push("config.json");
	let spec: runtime::Spec = runtime::SpecBuilder::default()
		.process(
			runtime::ProcessBuilder::default()
				.args(args)
				.build()
				.unwrap(),
		)
		.build()
		.unwrap();
	spec.save(&config_file.to_str().unwrap())
		.expect("Unable to write new specification file");
}
