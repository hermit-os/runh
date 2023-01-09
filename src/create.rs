use crate::hermit;
use crate::mounts;
use crate::rootfs;
use crate::state;
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
use std::os::unix::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::prelude::CommandExt;
use std::os::unix::prelude::FromRawFd;
use std::os::unix::prelude::IntoRawFd;
use std::path::PathBuf;
use std::str::FromStr;

use crate::container::OCIContainer;

#[allow(clippy::too_many_arguments)]
pub fn create_container(
	project_dir: PathBuf,
	id: Option<&str>,
	bundle: Option<&str>,
	pidfile: Option<&str>,
	console_socket: Option<&str>,
	hermit_env: Option<&str>,
	debug_config: bool,
	child_log_level: &str,
) {
	let _ = std::fs::create_dir(&project_dir);

	let container_dir = rootfs::resolve_in_rootfs(&PathBuf::from(id.unwrap()), &project_dir);
	std::fs::create_dir(container_dir.clone()).expect("Unable to create container directory");
	let container = OCIContainer::new(
		bundle.unwrap().to_string(),
		id.unwrap().to_string(),
		pidfile
			.unwrap_or(&(container_dir.to_str().unwrap().to_owned() + "/containerpid"))
			.to_string(),
	);

	// write container to disk
	let spec_path = container_dir.join("container.json");
	let mut file = OpenOptions::new()
		.read(true)
		.write(true)
		.create_new(true)
		.open(spec_path)
		.expect("Unable to create container");
	file.write_all(serde_json::to_string(&container).unwrap().as_bytes())
		.unwrap();

	// link container bundle
	fs::symlink(PathBuf::from(bundle.unwrap()), container_dir.join("bundle"))
		.expect("Unable to symlink bundle into project dir!");

	// write container to root dir
	if debug_config {
		let spec_path_backup = project_dir.join(format!("container-{}.json", id.unwrap()));
		let mut file = OpenOptions::new()
			.write(true)
			.create(true)
			.open(spec_path_backup)
			.expect("Unable to write spec to backup file!");
		file.write_all(serde_json::to_string(&container).unwrap().as_bytes())
			.unwrap();
	}

	// find rootfs
	let bundle_rootfs_path = PathBuf::from(&container.spec().root().as_ref().unwrap().path());
	let bundle_rootfs_path_abs = std::fs::canonicalize(if bundle_rootfs_path.is_absolute() {
		bundle_rootfs_path
	} else {
		PathBuf::from(bundle.unwrap()).join(bundle_rootfs_path)
	})
	.expect("Could not parse path to rootfs!");

	//Check for args[0] and detect hermit container
	let exec_args = &container
		.spec()
		.process()
		.as_ref()
		.unwrap()
		.args()
		.as_ref()
		.unwrap();

	let exec_path_rel = PathBuf::from(
		exec_args
			.get(0)
			.expect("Container spec does not contain any args!"),
	);
	let exec_path_abs = rootfs::resolve_in_rootfs(&exec_path_rel, &bundle_rootfs_path_abs);
	let is_hermit_container = if exec_path_abs.exists() {
		hermit::is_hermit_app(&exec_path_abs)
	} else {
		warn!("Could not find args-executable at current point in lifecycle. We will check again later, but hermit executables will NOT be detected!");
		false
	};
	if is_hermit_container {
		info!("Detected RustyHermit executable. Creating container in hermit mode!");
		//Setup hermit environment
		hermit::prepare_environment(&project_dir, &hermit_env);
	}

	//Setup exec fifo
	let fifo_location = container_dir.join("exec.fifo");
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
		.mode(0o0)
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
	});

	//Setup file system
	let mut rootfs_path_abs = bundle_rootfs_path_abs.clone();
	if is_hermit_container {
		let overlay_root = container_dir.join("rootfs");
		let overlay_workdir = overlay_root.join("work");
		let overlay_upperdir = overlay_root.join("diff");
		let overlay_mergeddir = overlay_root.join("merged");
		mounts::create_all_dirs(&overlay_workdir);
		mounts::create_all_dirs(&overlay_upperdir);
		mounts::create_all_dirs(&overlay_mergeddir);
		let datastr = format!(
			"lowerdir={}:{},upperdir={},workdir={}",
			hermit::get_environment_path(&project_dir, &hermit_env)
				.as_os_str()
				.to_str()
				.unwrap(),
			bundle_rootfs_path_abs.as_os_str().to_str().unwrap(),
			overlay_upperdir.as_os_str().to_str().unwrap(),
			overlay_workdir.as_os_str().to_str().unwrap()
		);
		nix::mount::mount::<str, PathBuf, str, str>(
			Some("overlay"),
			&overlay_mergeddir,
			Some("overlay"),
			nix::mount::MsFlags::empty(),
			Some(datastr.as_str()),
		)
		.unwrap_or_else(|err| panic!("Could not create overlay-fs at {:?}: {}", overlay_root, err));
		rootfs_path_abs = std::fs::canonicalize(overlay_mergeddir).unwrap();
	}

	//Pass spec file
	let mut config = std::path::PathBuf::from(bundle.unwrap().to_string());
	config.push("config.json");
	let spec_file = OpenOptions::new()
		.read(true)
		.write(false)
		.custom_flags(libc::O_CLOEXEC)
		.open(config)
		.expect("Could not open spec file!");

	let mut child_fd_mappings = vec![
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
	];

	//Setup console socket
	let socket_fds = if let Some(console_socket_path) = console_socket {
		let stream = UnixStream::connect(PathBuf::from(console_socket_path)).unwrap_or_else(|_| {
			panic!(
				"Could not connect to socket named by console-socket path at {}",
				console_socket_path
			)
		});
		let sock_stream_fd = stream.into_raw_fd();
		//let socket_fd_copy =
		//	nix::unistd::dup(sock_stream_fd).expect("Could not duplicate unix stream fd!");
		child_fd_mappings.push(FdMapping {
			parent_fd: sock_stream_fd,
			child_fd: 7,
		});
		Some(sock_stream_fd)
	} else {
		None
	};

	let _ = std::process::Command::new("/proc/self/exe")
		.arg("-l")
		.arg(child_log_level)
		.arg("--log-format")
		.arg("json")
		.arg("init")
		.stdin(std::process::Stdio::null())
		.fd_mappings(child_fd_mappings)
		.expect("Unable to pass fifo fd to child!")
		.env("RUNH_FIFOFD", "3")
		.env("RUNH_INITPIPE", "4")
		.env("RUNH_SPEC_FILE", "5")
		.env("RUNH_LOG_PIPE", "6")
		.env("RUNH_CONSOLE", "7")
		.env("RUNH_HERMIT_CONTAINER", is_hermit_container.to_string())
		.spawn()
		.expect("Unable to spawn runh init process");

	debug!("Started init process. Closing child fds in create process.");
	nix::unistd::close(child_socket_fd).expect("Could not close child_socket_fd!");
	nix::unistd::close(child_log_fd).expect("Could not close child_log_fd!");
	if let Some(stream_fd) = socket_fds {
		nix::unistd::close(stream_fd).expect("Could not close console stream_fd!");
	}

	debug!("Waiting for first message from child...");
	let mut init_pipe = unsafe { File::from_raw_fd(parent_socket_fd) };
	let mut buffer: [u8; 1] = [1];
	init_pipe
		.read_exact(&mut buffer)
		.expect("Could not read from init pipe!");
	debug!("Read from init pipe: {}", buffer[0]);

	//send rootfs path to child

	let rootfs_path_str = rootfs_path_abs
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
		.write_all(&rootfs_path_str.len().to_le_bytes())
		.expect("Could not write rootfs-path size to init pipe!");

	init_pipe
		.write_all(rootfs_path_str.as_bytes())
		.expect("Could not write rootfs-path to init pipe!");

	//send bundle rootfs path to child
	if is_hermit_container {
		let bundle_rootfs_path_str = bundle_rootfs_path_abs
			.as_os_str()
			.to_str()
			.expect("Could not convert rootfs-path to string!")
			.to_string();

		debug!(
			"Write bundle rootfs path {} (lenght {}) to init-pipe!",
			bundle_rootfs_path_str,
			bundle_rootfs_path_str.len()
		);
		init_pipe
			.write_all(&bundle_rootfs_path_str.len().to_le_bytes())
			.expect("Could not write hermit env path size to init pipe!");

		init_pipe
			.write_all(bundle_rootfs_path_str.as_bytes())
			.expect("Could not write hermit env path to init pipe!");
	}

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
	let mut sig_buffer = [0u8];

	//Running pre-start hooks
	debug!("Waiting for runh init to request prestart hooks");

	init_pipe
		.read_exact(&mut sig_buffer)
		.expect("Could not read from init pipe!");
	if sig_buffer[0] != crate::consts::INIT_REQ_PRESTART_HOOKS {
		panic!(
			"Received invalid signal from runh init! Expected {:x}, got {:x}",
			crate::consts::INIT_REQ_PRESTART_HOOKS,
			sig_buffer[0]
		);
	}

	let state_location = container_dir.join("created");
	let mut state_file = OpenOptions::new()
		.read(true)
		.write(true)
		.create(true)
		.open(state_location)
		.expect("Could not create state-file in container dir!");
	write!(state_file, "{}", pid).expect("Could not write pid to state-file!");

	debug!("Running prestart hooks...");
	if let Some(hooks) = container.spec().hooks().as_ref() {
		let state = state::State {
			version: String::from(crate::consts::OCI_STATE_VERSION),
			id: container.id().clone(),
			status: String::from("created"),
			pid: Some(pid),
			bundle: container.bundle().clone(),
			annotations: container.spec().annotations().clone(),
		};

		if let Some(prestart_hooks) = hooks.prestart().as_ref() {
			for hook in prestart_hooks {
				let mut cmd = std::process::Command::new(hook.path());
				if let Some(args) = &hook.args() {
					if !args.is_empty() {
						cmd.arg0(&args[0]);
					}
					if args.len() > 1 {
						cmd.args(&args[1..]);
					}
				}
				if let Some(env) = &hook.env() {
					for var in env {
						let (name, value) = var.split_once('=').unwrap_or_else(|| {
							panic!("Could not parse environment variable: {}", var)
						});
						cmd.env(name, value);
					}
				}
				if hook.timeout().is_some() {
					warn!("The timeout set for prestart hook {:?} is currently unimplemented and will be ignored!", hook.path());
				}
				cmd.stderr(std::process::Stdio::piped());
				cmd.stdin(std::process::Stdio::piped());
				let mut child = cmd.spawn().unwrap_or_else(|_| {
					panic!("Unable to spawn prestart hook process {:?}", hook.path())
				});
				write!(
					child.stdin.take().unwrap(),
					"{}",
					serde_json::to_string(&state).unwrap()
				)
				.expect("Could not write container state to hook process stdin!");

				let ret = child.wait_with_output().unwrap();
				if !ret.status.success() {
					panic!(
						"prestart hook {:?} returned exit status {}. Stderr: {}",
						hook.path(),
						ret.status,
						String::from_utf8(ret.stderr).unwrap()
					);
				}
			}
		}
	}

	init_pipe
		.write_all(&[crate::consts::CREATE_ACK_PRESTART_HOOKS])
		.expect("Unable to write to init-pipe!");

	//Waiting for init
	debug!("Waiting for runh init to get ready to execv!");

	if let Err(x) = init_pipe.read_exact(&mut sig_buffer) {
		log_forwarder.join().expect("Log forwarder did panic!");
		panic!("Could not read from init-pipe! Init probably died: {}", x);
	} else if sig_buffer[0] == crate::consts::INIT_READY_TO_EXECV {
		info!("Runh init ran successfully and is now ready to execv. Waiting for log pipe to close...");
		log_forwarder.join().expect("Log forwarder did panic!");
	} else {
		panic!("Received invalid signal from runh init!");
	}
}
