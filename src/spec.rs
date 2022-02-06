use oci_spec::runtime;
use std::fs;
use std::path;

pub fn create_spec(bundle: Option<&str>) {
	let dir = fs::canonicalize(path::PathBuf::from(bundle.unwrap()))
		.expect("Unable to determine absolute bundle path");
	let rootfs = path::PathBuf::from("/");
	let mut config_file = dir.clone();
	config_file.push("config.json");
	let cgroups_path = "/sys/fs/cgroup";

	let spec: runtime::Spec = runtime::SpecBuilder::default()
		.version("1.0.2".to_string())
		.hostname("hermit".to_string())
		.process(
			runtime::ProcessBuilder::default()
				.terminal(true)
				.console_size(
					runtime::BoxBuilder::default()
						.width(0u64)
						.height(0u64)
						.build()
						.unwrap(),
				)
				.user(
					runtime::UserBuilder::default()
						.uid(0u32)
						.gid(0u32)
						.username("root".to_string())
						.additional_gids(Vec::new())
						.build()
						.unwrap(),
				)
				.args(vec!["sh".to_string()])
				.env(vec![
					"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
				])
				.cwd("/".to_string())
				.capabilities(
					runtime::LinuxCapabilitiesBuilder::default()
						.build()
						.unwrap(),
				)
				.rlimits(Vec::new())
				.no_new_privileges(false)
				.apparmor_profile("".to_string())
				.oom_score_adj(0)
				.selinux_label("".to_string())
				.build()
				.unwrap(),
		)
		.root(
			runtime::RootBuilder::default()
				.path(rootfs.to_str().unwrap().to_string())
				.readonly(true)
				.build()
				.unwrap(),
		)
		.linux(
			runtime::LinuxBuilder::default()
				.cgroups_path(cgroups_path.to_string())
				.build()
				.unwrap(),
		)
		.build()
		.unwrap();

	spec.save(&config_file.to_str().unwrap())
		.expect("Unable to write new specification file");
}
