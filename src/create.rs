use command_fds::{CommandFdExt, FdMapping};
use nix::sys::socket;
use nix::sys::socket::SockFlag;
use nix::sys::stat::Mode;
use nix::unistd::Gid;
use nix::unistd::Uid;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::FromRawFd;

use crate::container::OCIContainer;

pub fn create_container(id: Option<&str>, bundle: Option<&str>, pidfile: Option<&str>) {
	let mut path = crate::get_project_dir();

	let _ = std::fs::create_dir(path.clone());

	path.push(id.unwrap());
	std::fs::create_dir(path.clone()).expect("Unable to create container directory");
	let container = OCIContainer::new(
		bundle.unwrap().to_string(),
		id.unwrap().to_string(),
		pidfile
			.unwrap_or(&(path.to_str().unwrap().to_owned() + "/containerpid"))
			.to_string(),
	);

	// write container to disk
	let spec_path = path.join("container.json");
	let mut file = OpenOptions::new()
		.read(true)
		.write(true)
		.create_new(true)
		.open(&spec_path)
		.expect("Unable to create container");
	file.write_all(serde_json::to_string(&container).unwrap().as_bytes())
		.unwrap();

	// let h = cgroups_rs::hierarchies::auto();
	// let cgroup: Cgroup = CgroupBuilder::new(&("hermit_".to_owned() + id.unwrap()))
	// 	.memory()
	// 	.done()
	// 	.cpu()
	// 	.shares(100)
	// 	.done()
	// 	.devices()
	// 	.device(
	// 		-1,
	// 		-1,
	// 		DeviceType::All,
	// 		false,
	// 		vec![
	// 			DevicePermissions::Read,
	// 			DevicePermissions::Write,
	// 			DevicePermissions::MkNod,
	// 		],
	// 	)
	// 	.device(
	// 		5,
	// 		1,
	// 		DeviceType::Char,
	// 		true,
	// 		vec![DevicePermissions::Read, DevicePermissions::Write],
	// 	)
	// 	.device(
	// 		1,
	// 		5,
	// 		DeviceType::Char,
	// 		true,
	// 		vec![DevicePermissions::Read, DevicePermissions::Write],
	// 	)
	// 	.device(
	// 		1,
	// 		3,
	// 		DeviceType::Char,
	// 		true,
	// 		vec![DevicePermissions::Read, DevicePermissions::Write],
	// 	)
	// 	.device(
	// 		1,
	// 		8,
	// 		DeviceType::Char,
	// 		true,
	// 		vec![DevicePermissions::Read, DevicePermissions::Write],
	// 	)
	// 	.device(
	// 		1,
	// 		9,
	// 		DeviceType::Char,
	// 		true,
	// 		vec![DevicePermissions::Read, DevicePermissions::Write],
	// 	)
	// 	.done()
	// 	.network()
	// 	.done()
	// 	.blkio()
	// 	.done()
	// 	.build(h);

	debug!(
		"Create container with uid {}, gid {}",
		container.spec().process.as_ref().unwrap().user.uid,
		container.spec().process.as_ref().unwrap().user.gid
	);

	//Setup exec fifo
	let fifo_location = path.join("exec.fifo");
	let old_mask = Mode::from_bits_truncate(0o000);
	nix::unistd::mkfifo(&fifo_location, Mode::from_bits_truncate(0o644))
		.expect("Could not create fifo!");

	let _ = nix::sys::stat::umask(old_mask);
	nix::unistd::chown(
		&fifo_location,
		Some(Uid::from_raw(0)),
		Some(Gid::from_raw(0)),
	)
	.expect("could not call chown!");

	let fifo = OpenOptions::new()
		.custom_flags(libc::O_PATH | libc::O_CLOEXEC)
		.read(true)
		.write(false)
		.mode(0)
		.open(&fifo_location)
		.expect("Could not open fifo!");

	//Setup init pipe
	let (parent_socket_fd, child_socket_fd) = socket::socketpair(
		socket::AddressFamily::Unix,
		socket::SockType::Stream,
		None,
		SockFlag::SOCK_CLOEXEC,
	)
	.expect("Could not create socket pair for init pipe!");

	let _ = std::process::Command::new("/proc/self/exe")
		.arg("init")
		.fd_mappings(vec![
			FdMapping {
				parent_fd: fifo.as_raw_fd(),
				child_fd: 3,
			},
			FdMapping {
				parent_fd: child_socket_fd,
				child_fd: 4,
			},
		])
		.expect("Unable to pass fifo fd to child!")
		.env("RUNH_FIFOFD", "3")
		.env("RUNH_INITPIPE", "4")
		.spawn()
		.expect("Unable to spawn runh init process");

	debug!("Started init process. Waiting for first message...");
	let mut init_pipe = unsafe { File::from_raw_fd(parent_socket_fd) };
	let mut buffer: [u8; 1] = [1];
	init_pipe
		.read_exact(&mut buffer)
		.expect("Could not read from init pipe!");
	debug!("Read from init pipe: {}", buffer[0]);
}
