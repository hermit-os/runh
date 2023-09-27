use oci_spec::runtime;
use std::path;

pub fn create_spec(bundle: path::PathBuf, args: Vec<String>) {
	let mut config_file = bundle;
	config_file.push("config.json");
	let mut root = runtime::Root::default();
	root.set_readonly(false.into());
	let spec: runtime::Spec = runtime::SpecBuilder::default()
		.process(
			runtime::ProcessBuilder::default()
				.args(args)
				.build()
				.unwrap(),
		)
		.root(root)
		.build()
		.unwrap();
	spec.save(config_file.to_str().unwrap())
		.expect("Unable to write new specification file");
}
