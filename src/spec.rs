use std::fs::File;
use std::io::prelude::*;

use crate::cri::runtime;

pub fn create_spec(dir: std::path::PathBuf) {
	let mut rootfs = dir.clone();
	rootfs.push("rootfs");
	let mut config_file = dir.clone();
	config_file.push("config.json");
	let cgroups_path = "/sys/fs/cgroup";
	let spec: runtime::Spec = runtime::SpecBuilder::default()
		.version("1.0.2")
		.hostname("hermit")
		.root(
			runtime::RootBuilder::default()
				.path(rootfs)
				.readonly(true)
				.build()
				.expect("Unable to create rootfs"),
		)
		.linux(
			runtime::LinuxBuilder::default()
				.cgroups_path(cgroups_path)
				.build()
				.expect("Unable to create platform configuration"),
		)
		.build()
		.expect("Unable to create spec");

	let mut file = File::create(config_file).expect("Unable to create file");
	write!(file, "{:?}", spec).expect("Unable to write new specification file");
}
