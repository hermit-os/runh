use crate::container::OCIContainer;
use goblin::elf;
use goblin::elf64::header::EI_OSABI;
use std::fs;
use std::io::{Read, Write};

pub fn start_container(id: Option<&str>) {
	let mut path = crate::get_project_dir();
	path.push(id.unwrap());
	path.push("container.json");

	if let Ok(mut file) = fs::OpenOptions::new().read(true).write(false).open(path) {
		let mut contents = String::new();
		file.read_to_string(&mut contents)
			.expect("Unable to read container specification");

		// Do we have a valid container?
		if let Ok(container) = serde_json::from_str::<OCIContainer>(&contents) {
			let host = container
				.spec()
				.hostname
				.as_ref()
				.unwrap_or(&"hermit".to_string())
				.clone();
			let rootfs = container.bundle().to_owned() + "/rootfs";

			debug!("Bundle at {}", container.bundle());
			debug!(
				"Run container with uid {}, gid {}",
				container.spec().process.as_ref().unwrap().user.uid,
				container.spec().process.as_ref().unwrap().user.gid
			);

			/*let mut mount_cmds = Vec::new();
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
			debug!("mount cmds {:?}", mount_cmds);*/

			let command = container
				.spec()
				.process
				.as_ref()
				.unwrap()
				.args
				.as_ref()
				.unwrap()
				.get(0)
				.unwrap();
			let buffer = fs::read(rootfs.clone() + "/" + command).unwrap();
			let elf = elf::Elf::parse(&buffer).unwrap();
			let start = if elf.header.e_ident[EI_OSABI] == 0xFF {
				format!("qemu-system-x86_64 -display none -smp 1 -m 64M -serial stdio -kernel /hermit/rusty-loader -initrd {} -cpu qemu64,apic,fsgsbase,rdtscp,xsave,fxsr,rdrand", command)
			} else {
				command.to_string()
			};

			let init_script = format!(
				"mount --bind {} {}; cd {} ; pivot_root . mnt; PATH=/bin:/sbin:/usr/bin:/usr/sbin:$PATH; umount -l /mnt ; mount -t tmpfs none /tmp ; hostname {} ; {}",
				rootfs, rootfs, rootfs, host, start
			);
			let jail = format!("cpu,memory,blkio,devices,freezer:/hermit_{}", id.unwrap());

			debug!("Container uses host name \"{}\"", host);
			debug!("Init script: {}", init_script);
			let mut child = std::process::Command::new("cgexec")
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
				.arg("--uts")
				.arg("/bin/bash")
				.arg("-c")
				.arg(init_script)
				.spawn()
				.expect("Unable to spawn process");

			// store pid into pidfile
			{
				let mut pidfile = fs::OpenOptions::new()
					.read(true)
					.write(true)
					.create_new(true)
					.open(container.pidfile())
					.expect("Unable to create container");
				let pidstr = format!("{}", child.id());
				pidfile
					.write_all(pidstr.as_bytes())
					.expect("Unable to store pid");
			}

			child.wait().expect("Unshare failed");
		}
	} else {
		println!("Container `{}` doesn't exists", id.unwrap());
	}
}
