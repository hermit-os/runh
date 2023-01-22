use oci_spec::runtime;
use std::path;

pub fn create_spec(bundle: path::PathBuf, args: Vec<String>) {
	let mut config_file = bundle;
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
	spec.save(config_file.to_str().unwrap())
		.expect("Unable to write new specification file");
}
