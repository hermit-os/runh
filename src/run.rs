use crate::container::OCIContainer;
use std::fs;
use std::io::Read;
use std::path::Path;

fn touch(path: &Path) {
	match fs::OpenOptions::new().create(true).write(true).open(path) {
		Ok(_) => {}
		Err(e) => warn!("Unable to touch file: {}", e),
	}
}

pub fn run_container(id: Option<&str>) {
	let mut path = crate::get_project_dir();
	path.push(id.unwrap());
	// path to the container specification
	let container_dir = path.clone();
	path.push("container.json");

	if let Ok(mut file) = fs::OpenOptions::new().read(true).write(false).open(path) {
		let mut contents = String::new();
		file.read_to_string(&mut contents)
			.expect("Unable to read container specification");

		// Do we have a valid container?
		if let Ok(container) = serde_json::from_str::<OCIContainer>(&contents) {
			let mut uts = container_dir.clone();
			uts.push("uts");
			/*let mut mount = container_dir.clone();
			mount.push("mount");
			let mut pid = container_dir.clone();
			pid.push("pid");
			let mut ipc = container_dir.clone();
			ipc.push("ipc");*/

			touch(&uts);
			/*touch(&mount);
			touch(&pid);
			touch(&ipc);*/

			/*let uts_arg = format!("--uts={}", uts.into_os_string().to_str().unwrap());
			let mount_arg = format!("--mount={}", mount.into_os_string().to_str().unwrap());
			let pid_arg = format!("--pid={}", pid.into_os_string().to_str().unwrap());
			let ipc_arg = format!("--ipc={}", ipc.into_os_string().to_str().unwrap());*/
			let uts_arg = format!("--uts={}", uts.into_os_string().to_str().unwrap());
			let mount_arg = format!("--mount");
			let pid_arg = format!("--pid");
			let ipc_arg = format!("--ipc");
			let fork_arg = format!("--fork");
			let host = container
				.spec()
				.hostname()
				.as_ref()
				.unwrap_or(&"hermit".to_string())
				.clone();

			let mut mount_cmds = Vec::new();
			if let Some(mount_points) = container.spec().mounts() {
				for mount in mount_points.iter() {
					let cmd = format!(
						"mkdir -p {} ; mount -o {} {} {}",
						mount.destination().to_str().unwrap(),
						mount.options().as_ref().unwrap().join(","),
						mount.source().as_ref().unwrap().to_str().unwrap(),
						mount.destination().to_str().unwrap()
					);
					mount_cmds.push(cmd);
				}
			}
			debug!("mount cmds {:?}", mount_cmds);

			let init_script = format!(
				"mount --make-rprivate / ; mount -t tmpfs none /home ; mount -t tmpfs none /tmp mount -t tmpfs none /sys ; mount -t tmpfs none /var/log; {} ; ls -la / ; mount ; hostname {}",
				mount_cmds.join(" ; "), host
			);

			debug!("Container uses host name \"{}\"", host);
			debug!("Init script: {}", init_script);
			std::process::Command::new("unshare")
				.arg("--map-root-user")
				.arg("--mount-proc")
				.arg(pid_arg)
				.arg(ipc_arg)
				.arg(fork_arg)
				.arg(mount_arg)
				.arg(uts_arg)
				.arg("/bin/bash")
				.arg("-c")
				.arg(init_script)
				.spawn()
				.expect("Unable to spawn process")
				.wait()
				.expect("Unshare failed");
		}
	} else {
		println!("Container `{}` doesn't exists", id.unwrap());
	}
}
