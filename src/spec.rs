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

	let spec: runtime::Spec = runtime::Spec {
		version: "1.0.2".to_string(),
		hostname: Some("hermit".to_string()),
		process: Some(runtime::Process {
			terminal: Some(true),
			console_size: Some(runtime::Box {
				width: 0,
				height: 0,
			}),
			user: runtime::User {
				uid: 0,
				gid: 0,
				username: Some("root".to_string()),
				additional_gids: Some(Vec::new()),
			},
			args: Some(vec!["sh".to_string()]),
			env: Some(vec![
				"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
			]),
			cwd: "/".to_string(),
			capabilities: Some(runtime::LinuxCapabilities {
				bounding: None,
				effective: None,
				inheritable: None,
				permitted: None,
				ambient: None,
			}),
			rlimits: Some(Vec::new()),
			no_new_privileges: Some(false),
			apparmor_profile: Some("".to_string()),
			oom_score_adj: Some(0),
			selinux_label: Some("".to_string()),
		}),
		root: Some(runtime::Root {
			path: rootfs.to_str().unwrap().to_string(),
			readonly: Some(true),
		}),
		mounts: None,
		hooks: None,
		annotations: None,
		linux: Some(runtime::Linux {
			uid_mappings: None,
			gid_mappings: None,
			sysctl: None,
			resources: None,
			cgroups_path: Some(cgroups_path.to_string()),
			namespaces: None,
			devices: None,
			seccomp: None,
			rootfs_propagation: None,
			masked_paths: None,
			readonly_paths: None,
			mount_label: None,
			intel_rdt: None,
		}),
		solaris: None,
		windows: None,
		vm: None,
	};

	spec.save(&config_file.to_str().unwrap())
		.expect("Unable to write new specification file");
}
