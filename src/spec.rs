use std::fs;
use std::path;

pub fn create_spec(bundle: Option<&str>) {
	todo!("Convert code for spec creation to oci_spec library")
	/*let dir = fs::canonicalize(path::PathBuf::from(bundle.unwrap()))
		.expect("Unable to determine absolute bundle path");
	let rootfs = path::PathBuf::from("/");
	let mut config_file = dir.clone();
	config_file.push("config.json");
	let cgroups_path = "/sys/fs/cgroup";

	let spec: runtime::Spec = runtime::SpecBuilder::default()
		.version("1.0.2")
		.hostname("hermit")
		.process(
			runtime::ProcessBuilder::default()
				.terminal(true)
				.console_size(
					runtime::BoxBuilder::default()
						.build()
						.expect("Unable to create box"),
				)
				.user(
					runtime::UserBuilder::default()
						.uid(0u32)
						.gid(0u32)
						.umask(0o644u32)
						.additional_gids(Vec::new())
						.username("root")
						.build()
						.expect("Unable to create user"),
				)
				.args(vec!["sh".to_string()])
				.command_line("")
				.env(vec![
					"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
				])
				.cwd("/")
				.capabilities(
					runtime::LinuxCapabilitiesBuilder::default()
						.build()
						.expect("Unable to create capabilities"),
				)
				.rlimits(Vec::new())
				.no_new_privileges(false)
				.apparmor_profile("")
				.oom_score_adj(0)
				.selinux_label("")
				.build()
				.expect("Unable to create process informaion"),
		)
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

	spec.save(&config_file)
		.expect("Unable to write new specification file");*/
}
