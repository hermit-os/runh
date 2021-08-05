use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::prelude::{AsRawFd, IntoRawFd, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::{
	env,
	fs::File,
	os::unix::prelude::{FromRawFd, RawFd},
};

use crate::mounts;
use crate::namespaces;
use crate::{flags, paths, rootfs};
use capctl::prctl;
use nix::mount::MsFlags;
use nix::sched::CloneFlags;
use nix::sys::socket;
use nix::unistd::{Gid, Pid, Uid};
use oci_spec::runtime::{self, Spec};

#[derive(Clone, Copy, Debug, Default)]
struct SocketPair {
	parent: RawFd,
	child: RawFd,
}

impl From<(i32, i32)> for SocketPair {
	fn from(tuple: (i32, i32)) -> Self {
		SocketPair {
			parent: RawFd::from(tuple.0),
			child: RawFd::from(tuple.1),
		}
	}
}
#[derive(Clone, Copy, Debug)]
enum InitStage {
	PARENT,
	CHILD,
	GRANDCHILD,
}

#[derive(Debug)]
struct InitConfig {
	spec: Spec,
	cloneflags: CloneFlags,
	rootfs: String,
}

#[derive(Debug)]
struct SetupArgs {
	stage: InitStage,
	init_pipe: RawFd,
	parent_child_sync: SocketPair,
	parent_grandchild_sync: SocketPair,
	config: InitConfig,
}

#[repr(align(16))]
struct CloneArgs {
	stack: [u8; 16384],
	args: SetupArgs,
	child_func: Box<dyn Fn(SetupArgs) -> isize>,
}

pub fn init_container() {
	// This implements the init process functionality,
	// analogous to https://github.com/opencontainers/runc/blob/master/libcontainer/nsenter/nsexec.c

	// During this process, it:
	// - forks a child process
	// - unshares from the user namespaces
	// - unshares from all other requested namespaces
	// - creates a grandchild process in a new PID namespace
	// - reports back the child- and grandchild-PID to the create process
	// - Waits for the exec-fifo to open during the runh start call
	let pipe_fd: i32 = env::var("RUNH_INITPIPE")
		.expect("No init pipe given!")
		.parse()
		.expect("RUNH_INITPIPE was not an integer!");
	let mut init_pipe = unsafe { File::from_raw_fd(RawFd::from(pipe_fd)) };
	write!(init_pipe, "\0").expect("Unable to write to init-pipe!");

	let mut size_buffer = [0u8; std::mem::size_of::<usize>()];
	init_pipe
		.read_exact(&mut size_buffer)
		.expect("Could not read message size from init-pipe!");
	let message_size = usize::from_le_bytes(size_buffer);
	debug!("Rootfs-path lenght: {}", message_size);

	let mut rootfs_path_buffer = vec![0; message_size as usize];
	init_pipe
		.read_exact(&mut rootfs_path_buffer)
		.expect("Could not read rootfs-path from init pipe!");
	let rootfs_path =
		String::from_utf8(rootfs_path_buffer).expect("Could not parse rootfs-path as string!");
	debug!("read rootfs from init_pipe: {}", rootfs_path);

	debug!("read config from spec file");
	let spec_fd: i32 = env::var("RUNH_SPEC_FILE")
		.expect("No spec file given!")
		.parse()
		.expect("RUNH_SPEC_FILE was not an integer!");
	let spec_file = unsafe { File::from_raw_fd(RawFd::from(spec_fd)) };
	let spec: Spec = serde_json::from_reader(&spec_file).expect("Unable to read spec file!");

	debug!("generate clone-flags");
	let cloneflags = if let Some(namespaces) = &spec.linux.as_ref().unwrap().namespaces {
		flags::generate_cloneflags(namespaces)
	} else {
		CloneFlags::empty()
	};

	debug!("set process as non-dumpable");
	prctl::set_dumpable(false).expect("Could not set process as non-dumpable!");

	debug!("create child sync pipe");
	let parent_child_sync = SocketPair::from(
		socket::socketpair(
			socket::AddressFamily::Unix,
			socket::SockType::Stream,
			None,
			socket::SockFlag::SOCK_CLOEXEC,
		)
		.expect("Could not create parent-child socket pair for init pipe!"),
	);

	debug!("create grandchild sync pipe");
	let parent_grandchild_sync = SocketPair::from(
		socket::socketpair(
			socket::AddressFamily::Unix,
			socket::SockType::Stream,
			None,
			socket::SockFlag::SOCK_CLOEXEC,
		)
		.expect("Could not create parent-grandchild socket pair for init pipe!"),
	);

	debug!("jump into init_stage");
	init_stage(SetupArgs {
		stage: InitStage::PARENT,
		init_pipe: init_pipe.into_raw_fd(),
		parent_child_sync,
		parent_grandchild_sync,
		config: InitConfig {
			spec,
			cloneflags,
			rootfs: rootfs_path,
		},
	});
}

fn clone_process(mut args: Box<CloneArgs>) -> nix::unistd::Pid {
	extern "C" fn callback(data: *mut CloneArgs) -> i32 {
		let cb: Box<CloneArgs> = unsafe { Box::from_raw(data) };
		(*cb.child_func)(cb.args) as i32
	}

	let res = unsafe {
		let combined = nix::sched::CloneFlags::CLONE_PARENT.bits() | libc::SIGCHLD;
		let ptr = args.stack.as_mut_ptr().offset(args.stack.len() as isize);
		let ptr_aligned = ptr.offset((ptr as usize % 16) as isize * -1);
		libc::clone(
			std::mem::transmute(callback as extern "C" fn(*mut CloneArgs) -> i32),
			ptr_aligned as *mut libc::c_void,
			combined,
			Box::into_raw(args) as *mut _ as *mut libc::c_void,
		)
	};

	nix::errno::Errno::result(res)
		.map(nix::unistd::Pid::from_raw)
		.expect("Could not clone parent process!")
}

fn init_stage(args: SetupArgs) -> isize {
	match args.stage {
		InitStage::PARENT => {
			debug!("enter init_stage parent");
			// Setting the name is just for debugging purposes so it doesnt cause problems if it fails
			let _ = prctl::set_name("runh:PARENT");
			let child_pid = clone_process(Box::new(CloneArgs {
				stack: [0; 16384],
				args: SetupArgs {
					stage: InitStage::CHILD,
					init_pipe: args.init_pipe,
					parent_child_sync: args.parent_child_sync,
					parent_grandchild_sync: args.parent_grandchild_sync,
					config: args.config,
				},
				child_func: Box::new(init_stage),
			}));
			debug!("Created child with pid {}", child_pid);
			debug!("Wait for synchronization with children!");

			let mut pid_buffer = [0; 4];
			let mut child_sync_pipe = unsafe { File::from_raw_fd(args.parent_child_sync.parent) };
			debug!(
				"Listening on fd {} for grandchild pid",
				args.parent_child_sync.parent
			);
			child_sync_pipe
				.read_exact(&mut pid_buffer)
				.expect("could not synchronize with first child!");

			let received_pid = i32::from_le_bytes(pid_buffer);
			debug!("Received child PID: {}", received_pid);

			debug!("send child PID to runh create");
			let mut init_pipe = unsafe { File::from_raw_fd(RawFd::from(args.init_pipe)) };
			init_pipe
				.write(&pid_buffer)
				.expect("Unable to write to init-pipe!");
			return 0; // Exit parent
		}
		InitStage::CHILD => {
			debug!("enter init_stage child");
			let _ = prctl::set_name("runh:CHILD");
			if let Some(namespaces) = &args.config.spec.linux.as_ref().unwrap().namespaces {
				namespaces::join_namespaces(namespaces)
			}

			//TODO: Unshare user namespace if requested
			//TODO: Let parent setup uidmap/gidmap if a user namespace was joined

			nix::unistd::setresuid(Uid::from_raw(0), Uid::from_raw(0), Uid::from_raw(0))
				.expect("could not become root in user namespace!");

			// Unshare all other namespaces (except cgroup)
			debug!(
				"unshare namespaces with cloneflags {:?}",
				args.config.cloneflags
			);
			let mut flags = args.config.cloneflags.clone();
			flags.remove(CloneFlags::CLONE_NEWCGROUP);
			nix::sched::unshare(flags).expect("could not unshare non-user namespaces!");

			// Fork again into new PID-Namespace and send PID to parent
			let grandchild_pid: i32 = clone_process(Box::new(CloneArgs {
				stack: [0; 16384],
				args: SetupArgs {
					stage: InitStage::GRANDCHILD,
					init_pipe: args.init_pipe,
					parent_child_sync: args.parent_child_sync,
					parent_grandchild_sync: args.parent_grandchild_sync,
					config: args.config,
				},
				child_func: Box::new(init_stage),
			}))
			.into();

			// Send grandchild-PID to parent process
			debug!("writing PID to fd {}", args.parent_child_sync.child);
			let mut child_sync_pipe = unsafe { File::from_raw_fd(args.parent_child_sync.child) };
			let written_bytes = child_sync_pipe
				.write(&grandchild_pid.to_le_bytes())
				.expect("Could not write grandchild-PID to pipe!");
			debug!("Wrote {} bytes for grandchild-PID", written_bytes);
			return 0; // Exit child process
		}
		InitStage::GRANDCHILD => {
			debug!("enter init_stage grandchild");
			let _ = prctl::set_name("runh:INIT");
			debug!("Welcome to the container! This is PID {}", Pid::this());

			// Set SID, UID, GID
			let _ = nix::unistd::setsid().expect("Could not set session ID");
			nix::unistd::setuid(Uid::from_raw(0)).expect("Could not set user ID");
			nix::unistd::setgid(Gid::from_raw(0)).expect("Could not set group ID");

			// TODO: Call setgroups if !is_rootless_euid && is_setgroup (?)

			// Unshare Cgroup namespace if requested to
			if args.config.cloneflags.contains(CloneFlags::CLONE_NEWCGROUP) {
				// TODO: Synchronize with runh create for cgroup setup
				nix::sched::unshare(CloneFlags::CLONE_NEWCGROUP)
					.expect("could not unshare cgroups namespace!");
			}

			// In runc's case, this is the point where control is transferred back to the go runtime
			debug!("read config from spec file");
			let fifo_fd: i32 = env::var("RUNH_FIFOFD")
				.expect("No fifo fd given!")
				.parse()
				.expect("RUNH_FIFOFD was not an integer!");

			unsafe {
				libc::clearenv();
			}

			// Set environment variables found in the config
			if let Some(process) = &args.config.spec.process {
				if let Some(env) = &process.env {
					debug!("load environment variables from config");
					for var in env {
						let (name, value) = var.split_once("=").expect(
							format!("Could not parse environment variable: {}", var).as_str(),
						);
						std::env::set_var(name, value);
					}
				}
			}

			//TODO: Create new session keyring if requested
			//TODO: Setup network and routing

			//Setup devices, mountpoints and file system
			let mut mount_flags = MsFlags::empty();
			mount_flags.insert(MsFlags::MS_REC);
			mount_flags.insert(match args.config.spec.linux.as_ref().unwrap().rootfs_propagation.as_ref().and_then(|x| Some(x.as_str())) {
				Some("shared") => MsFlags::MS_SHARED,
				Some("slave") => MsFlags::MS_SLAVE,
				Some("private") => MsFlags::MS_PRIVATE,
				Some("unbindable") => MsFlags::MS_UNBINDABLE,
				Some(_) => panic!("Value of rootfsPropagation did not match any known option! Given value: {}", &args.config.spec.linux.as_ref().unwrap().rootfs_propagation.as_ref().unwrap()),
				None => MsFlags::MS_SLAVE
			});

			nix::mount::mount::<Option<&str>, str, Option<&str>, Option<&str>>(
				None,
				"/",
				None,
				mount_flags,
				None,
			)
			.expect(
				format!(
					"Could not mount rootfs with given MsFlags {:?}",
					mount_flags
				)
				.as_str(),
			);

			//TODO: Make parent mount private (?)
			let mut bind_mount_flags = MsFlags::empty();
			bind_mount_flags.insert(MsFlags::MS_BIND);
			bind_mount_flags.insert(MsFlags::MS_REC);

			let rootfs_path = PathBuf::from(args.config.rootfs);

			nix::mount::mount::<PathBuf, PathBuf, str, Option<&str>>(
				Some(&rootfs_path),
				&rootfs_path,
				Some("bind"),
				bind_mount_flags,
				None,
			)
			.expect(format!("Could not bind-mount rootfs at {:?}", rootfs_path).as_str());

			if let Some(mounts) = args.config.spec.mounts {
				mounts::configure_mounts(
					&mounts,
					&rootfs_path,
					args.config.spec.linux.unwrap().mount_label,
				);
			}

			nix::unistd::chdir(&rootfs_path).expect(
				format!(
					"Could not change directory to rootfs path {:?}",
					rootfs_path
				)
				.as_str(),
			);

			//TODO: Run create hooks

			if args.config.cloneflags.contains(CloneFlags::CLONE_NEWNS) {
				rootfs::pivot_root(&rootfs_path);
			} else {
				nix::unistd::chroot(".").expect("Could not chroot into current directory!");
				nix::unistd::chdir("/").expect("Could not chdir to / after chroot!");
			}

			//TODO: setup /dev/null

			let cwd = &args.config.spec.process.as_ref().unwrap().cwd;
			if !cwd.is_empty() {
				mounts::create_all_dirs(&PathBuf::from(cwd));
			}

			//TODO: Setup console

			//Finalize rootfs
			if args.config.cloneflags.contains(CloneFlags::CLONE_NEWNS) {
				//TODO: Remount /dev as ro if requested

				if let Some(root) = args.config.spec.root {
					if root.readonly.unwrap_or(false) {
						rootfs::set_rootfs_read_only();
					}
				}
				let _ = nix::sys::stat::umask(nix::sys::stat::Mode::from_bits(0o022).unwrap());
			}

			if let Some(hostname) = args.config.spec.hostname {
				debug!("set hostname to {}", &hostname);
				nix::unistd::sethostname(hostname).expect("Could not set hostname!");
			}

			//TODO: Apply apparmor profile
			//TODO: Write sysctl keys
			//TODO: Manage readonly and mask paths

			// Set no_new_privileges
			if let Some(process) = &args.config.spec.process {
				if process.no_new_privileges.unwrap_or(false) {
					debug!("set no_new_privileges");
					prctl::set_no_new_privs().expect("Could not set no_new_privs flag!");
				}
			}

			let exec_args = args.config.spec.process.unwrap().args.unwrap();

			//Verify the args[0] executable exists
			let exec_path_rel = PathBuf::from(
				exec_args
					.get(0)
					.expect("Container spec does not contain any args!"),
			);
			let exec_path_abs = paths::find_in_path(exec_path_rel)
				.expect("Could not determine location of args-executable!");

			info!("Found args-executable: {:?}", exec_path_abs);

			//Tell runh create we are ready to execv
			let mut init_pipe = unsafe { File::from_raw_fd(RawFd::from(args.init_pipe)) };
			init_pipe
				.write(&[crate::consts::INIT_READY_TO_EXECV])
				.expect("Unable to write to init-pipe!");

			info!("Runh init setup complete. Now waiting for signal to execv!");

			let mut exec_fifo = OpenOptions::new()
				.custom_flags(libc::O_CLOEXEC)
				.read(false)
				.write(true)
				.open(format!("/proc/self/fd/{}", fifo_fd))
				.expect("Could not open exec fifo!");

			write!(exec_fifo, "\0").expect("Could not write to exec fifo!");

			debug!("Fifo was opened! Starting container process...");

			let error = std::process::Command::new(exec_path_abs)
				.arg0(exec_args.get(0).unwrap())
				.args(exec_args.get(1..).unwrap())
				.envs(std::env::vars())
				.exec();

			panic!("exec failed with error {}", error);
		}
	}
}
