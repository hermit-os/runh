use oci_spec::runtime;
use std::fs;
use std::path;

pub fn create_spec(bundle: Option<&str>) {
	let dir = fs::canonicalize(path::PathBuf::from(bundle.unwrap()))
		.expect("Unable to determine absolute bundle path");
	let mut config_file = dir.clone();
	config_file.push("config.json");
	let spec: runtime::Spec = Default::default();
	spec.save(&config_file.to_str().unwrap())
		.expect("Unable to write new specification file");
}
