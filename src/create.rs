use command_fds::{CommandFdExt, FdMapping};
use nix::fcntl::OFlag;
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
use std::path::PathBuf;
use std::str::FromStr;

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

	//Setup log pipe
	let (parent_log_fd, child_log_fd) =
		nix::unistd::pipe2(OFlag::O_CLOEXEC).expect("Could not create socket pair for log pipe!");
	let log_forwarder = std::thread::spawn(move || {
		let log_pipe = unsafe { std::fs::File::from_raw_fd(parent_log_fd) };
		let mut reader = std::io::BufReader::new(log_pipe);
		let mut buffer: Vec<u8> = vec![];
		while let Ok(bytes_read) = reader.read_until(b"}"[0], &mut buffer) {
			if bytes_read > 0 {
				if let Ok(log_entry) =
					serde_json::from_slice::<crate::logging::LogEntry>(buffer.as_slice())
				{
					match log::Level::from_str(log_entry.level.as_str()) {
						Ok(level) => log!(level, "[INIT] {}", log_entry.msg),
						Err(_) => info!("[INIT] {}", log_entry.msg),
					}
					buffer.clear();
				}
			} else {
				debug!("Read zero bytes from log pipe, closing forwarder...");
				break;
			}
		}
		debug!("Encountered error while reading from log pipe, closing forwarder...");
	});

	//Pass spec file
	let mut config = std::path::PathBuf::from(bundle.unwrap().to_string());
	config.push("config.json");
	let spec_file = File::open(config).expect("Could not open spec file!");

	let _ = std::process::Command::new("/proc/self/exe")
		.arg("-l")
		.arg("debug")
		.arg("--log-format")
		.arg("json")
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
			FdMapping {
				parent_fd: spec_file.as_raw_fd(),
				child_fd: 5,
			},
			FdMapping {
				parent_fd: child_log_fd,
				child_fd: 6,
			},
		])
		.expect("Unable to pass fifo fd to child!")
		.env("RUNH_FIFOFD", "3")
		.env("RUNH_INITPIPE", "4")
		.env("RUNH_SPEC_FILE", "5")
		.env("RUNH_LOG_PIPE", "6")
		.spawn()
		.expect("Unable to spawn runh init process");

	debug!("Started init process. Waiting for first message...");
	let mut init_pipe = unsafe { File::from_raw_fd(parent_socket_fd) };
	let mut buffer: [u8; 1] = [1];
	init_pipe
		.read_exact(&mut buffer)
		.expect("Could not read from init pipe!");
	debug!("Read from init pipe: {}", buffer[0]);

	let rootfs_path = PathBuf::from(bundle.unwrap().to_string() + "/rootfs");
	let rootfs_path_str = std::fs::canonicalize(rootfs_path)
		.expect("Could not parse path to rootfs!")
		.as_os_str()
		.to_str()
		.expect("Could not convert rootfs-path to string!")
		.to_string();
	debug!(
		"Write rootfs-path {} (lenght {}) to init-pipe!",
		rootfs_path_str,
		rootfs_path_str.len()
	);
	init_pipe
		.write(&(rootfs_path_str.len() as usize).to_le_bytes())
		.expect("Could not write rootfs-path size to init pipe!");

	init_pipe
		.write_all(rootfs_path_str.as_bytes())
		.expect("Could not write rootfs-path to init pipe!");

	debug!("Waiting for runh init to send grandchild PID");
	let mut pid_buffer = [0; 4];
	init_pipe
		.read_exact(&mut pid_buffer)
		.expect("Could not read from init pipe!");

	let pid = i32::from_le_bytes(pid_buffer);
	if let Some(pid_file_path) = pidfile {
		let mut file = std::fs::File::create(pid_file_path).expect("Could not create pid-File!");
		write!(file, "{}", pid).expect("Could not write to pid-file!");
	}
	debug!("Waiting for runh init to get ready to execv!");
	let mut sig_buffer = [0u8];
	init_pipe
		.read_exact(&mut sig_buffer)
		.expect("Could not read from init-pipe!");

	if sig_buffer[0] == crate::consts::INIT_READY_TO_EXECV {
		info!("Runh init ran successfully and is now ready to execv. Waiting for log pipe to close...");
		log_forwarder.join().expect("Log forwarder did panic!");
		return;
	} else {
		panic!("Received invalid signal from runh init!");
	}
}
