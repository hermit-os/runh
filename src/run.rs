use crate::container::OCIContainer;
use goblin::elf;
use goblin::elf64::header::EI_OSABI;
use std::fs;
use std::io::Read;
use std::path::Path;

/*fn touch(path: &Path) {
	match fs::OpenOptions::new().create(true).write(true).open(path) {
		Ok(_) => {}
		Err(e) => warn!("Unable to touch file: {}", e),
	}
}*/

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
			/*let mut uts = container_dir.clone();
			uts.push("uts");
			touch(&uts);*/

			let uts_arg = "--uts"; //format!("--uts={}", uts.into_os_string().to_str().unwrap());
			let host = container
				.spec()
				.hostname()
				.as_ref()
				.unwrap_or(&"hermit".to_string())
				.clone();

			debug!(
				"Run container with uid {}, gid {}",
				container.spec().process().as_ref().unwrap().user().uid(),
				container.spec().process().as_ref().unwrap().user().gid()
			);

			let mut mount_cmds = Vec::new();
			if let Some(mount_points) = container.spec().mounts() {
				for mount in mount_points.iter() {
					let cmd = format!(
						"mount -o {} {} {}",
						mount.options().as_ref().unwrap().join(","),
						mount.source().as_ref().unwrap().to_str().unwrap(),
						mount.destination().to_str().unwrap()
					);
					mount_cmds.push(cmd);
				}
			}
			debug!("mount cmds {:?}", mount_cmds);

			let command = container
				.spec()
				.process()
				.as_ref()
				.unwrap()
				.args()
				.as_ref()
				.unwrap()
				.get(0)
				.unwrap();
			let buffer = fs::read(command).unwrap();
			let elf = elf::Elf::parse(&buffer).unwrap();
			let start = if elf.header.e_ident[EI_OSABI] == 0xFF {
				format!("qemu-system-x86_64 -display none -smp 1 -m 64M -serial stdio -kernel /mnt/rusty-loader -initrd {} -cpu qemu64,apic,fsgsbase,rdtscp,xsave,fxsr,rdrand", command)
			} else {
				command.to_string()
			};

			let init_script = format!(
				"mount --make-rprivate / ; {} ; mount -t tmpfs none /home ; mount -t tmpfs none /root  ; mount -t tmpfs none /tmp ; mount -t tmpfs none /sys ; mount -t tmpfs none /var/log ; hostname {} ; {}",
				mount_cmds.join(" ; "), host, start
			);
			let jail = format!("cpu,memory,blkio,devices,freezer:/hermit_{}", id.unwrap());

			debug!("Container uses host name \"{}\"", host);
			debug!("Init script: {}", init_script);
			std::process::Command::new("cgexec")
				.arg("-g")
				.arg(jail)
				.arg("unshare")
				.arg("--map-root-user")
				.arg("--mount-proc")
				.arg("--pid")
				.arg("--ipc")
				.arg("--fork")
				.arg("--mount")
				.arg("--net")
				.arg("--user")
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
