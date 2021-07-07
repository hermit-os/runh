use std::io::{Read, Write};
use std::os::unix::prelude::{AsRawFd, IntoRawFd};
use std::{
	env,
	fs::File,
	os::unix::prelude::{FromRawFd, RawFd},
};

use capctl::prctl;
use nix::sched::{self, CloneFlags};
use nix::sys::socket;
use nix::unistd::{Pid, Uid};
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
}

#[derive(Debug)]
struct SetupArgs<'a> {
	stage: InitStage,
	init_pipe: RawFd,
	parent_child_sync: SocketPair,
	parent_grandchild_sync: SocketPair,
	config: &'a InitConfig,
}

#[repr(align(16))]
struct CloneArgs<'a> {
	stack: [u8; 4096],
	args: &'a SetupArgs<'a>,
	child_func: Box<dyn FnMut(&SetupArgs) -> isize + 'a>,
}

pub fn init_container() {
	// This implements the init process functionality,
	// analogous to https://github.com/opencontainers/runc/blob/master/libcontainer/nsenter/nsexec.c

	// During this process, it:
	// - forks a child process
	// - unshares from the user namespaces
	// - unshares from all other requested namespace
	// - creates a grandchild process in a new PID namespace
	// - reports back the child- and grandchild-PID to the create process
	// - Waits for the exec-fifo to open during the runh start call
	let pipe_fd: i32 = env::var("RUNH_INITPIPE")
		.expect("No init pipe given!")
		.parse()
		.expect("RUNH_INITPIPE was not an integer!");
	let mut init_pipe = unsafe { File::from_raw_fd(RawFd::from(pipe_fd)) };
	write!(init_pipe, "\0").expect("Unable to write to init-pipe!");

	debug!("read config from spec file");
	let spec_fd: i32 = env::var("RUNH_SPEC_FILE")
		.expect("No spec file given!")
		.parse()
		.expect("RUNH_SPEC_FILE was not an integer!");
	let spec_file = unsafe { File::from_raw_fd(RawFd::from(spec_fd)) };
	let config: Spec = serde_json::from_reader(&spec_file).expect("Unable to read spec file!");

	debug!("generate clone-flags");
	let cloneflags = if let Some(namespaces) = &config.linux.as_ref().unwrap().namespaces {
		generate_cloneflags(namespaces)
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
	init_stage(&SetupArgs {
		stage: InitStage::PARENT,
		init_pipe: init_pipe.into_raw_fd(),
		parent_child_sync,
		parent_grandchild_sync,
		config: &InitConfig {
			spec: config,
			cloneflags,
		},
	});
}

fn clone_process(mut args: CloneArgs) -> nix::unistd::Pid {
	extern "C" fn callback(data: *mut CloneArgs) -> i32 {
		let cb: &mut CloneArgs = unsafe { &mut *data };
		(*cb.child_func)(cb.args) as i32
	}

	let res = unsafe {
		let combined = sched::CloneFlags::CLONE_PARENT.bits() | libc::SIGCHLD;
		let ptr = args.stack.as_mut_ptr().offset(args.stack.len() as isize);
		let ptr_aligned = ptr.offset((ptr as usize % 16) as isize * -1);
		libc::clone(
			std::mem::transmute(callback as extern "C" fn(*mut CloneArgs) -> i32),
			ptr_aligned as *mut libc::c_void,
			combined,
			&mut args as *mut _ as *mut libc::c_void,
		)
	};

	nix::errno::Errno::result(res)
		.map(nix::unistd::Pid::from_raw)
		.expect("Could not clone parent process!")
}

fn init_stage(args: &SetupArgs) -> isize {
	match args.stage {
		InitStage::PARENT => {
			debug!("enter init_stage parent");
			// Setting the name is just for debugging purposes so it doesnt cause problems if it fails
			let _ = prctl::set_name("runh:PARENT");
			let child_pid = clone_process(CloneArgs {
				stack: [0; 4096],
				args: &SetupArgs {
					stage: InitStage::CHILD,
					init_pipe: args.init_pipe,
					parent_child_sync: args.parent_child_sync,
					parent_grandchild_sync: args.parent_grandchild_sync,
					config: args.config,
				},
				child_func: Box::new(init_stage),
			});
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
				join_namespaces(namespaces)
			}

			//TODO: Unshare user namespace if requested

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
			let grandchild_pid: i32 = clone_process(CloneArgs {
				stack: [0; 4096],
				args: &SetupArgs {
					stage: InitStage::GRANDCHILD,
					init_pipe: args.init_pipe,
					parent_child_sync: args.parent_child_sync,
					parent_grandchild_sync: args.parent_grandchild_sync,
					config: args.config,
				},
				child_func: Box::new(init_stage),
			})
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
			debug!("Welcome to the container! This is PID {}", Pid::this());
			loop {}
		}
	}
}

struct ConfiguredNamespace<'a>(File, &'a runtime::LinuxNamespace);

fn join_namespaces(namespaces: &Vec<runtime::LinuxNamespace>) {
	let mut configured_ns: Vec<ConfiguredNamespace> = Vec::new();
	for ns in namespaces {
		if let Some(path) = ns.path.as_ref() {
			configured_ns.push(ConfiguredNamespace(
				File::open(path).expect(
					format!(
						"failed to open {} for NS {}",
						ns.path.as_ref().unwrap(),
						ns.typ
					)
					.as_str(),
				),
				ns,
			));
		} else {
			debug!(
				"Namespace {} has no path, skipping in join_namespaces",
				ns.typ
			);
		}
	}

	for ns_config in &configured_ns {
		debug!("joining namespace {:?}", ns_config.1);
		let flags = match ns_config.1.typ {
			runtime::LinuxNamespaceType::cgroup => CloneFlags::CLONE_NEWCGROUP,
			runtime::LinuxNamespaceType::ipc => CloneFlags::CLONE_NEWIPC,
			runtime::LinuxNamespaceType::mount => CloneFlags::CLONE_NEWNS,
			runtime::LinuxNamespaceType::network => CloneFlags::CLONE_NEWNET,
			runtime::LinuxNamespaceType::pid => CloneFlags::CLONE_NEWPID,
			runtime::LinuxNamespaceType::user => CloneFlags::CLONE_NEWUSER,
			runtime::LinuxNamespaceType::uts => CloneFlags::CLONE_NEWUTS,
		};
		nix::sched::setns(ns_config.0.as_raw_fd(), flags)
			.expect(format!("Failed to join NS {:?}", ns_config.1).as_str());
	}
}

fn generate_cloneflags(namespaces: &Vec<runtime::LinuxNamespace>) -> CloneFlags {
	let mut result = CloneFlags::empty();
	for ns in namespaces {
		let flag = match ns.typ {
			runtime::LinuxNamespaceType::cgroup => CloneFlags::CLONE_NEWCGROUP,
			runtime::LinuxNamespaceType::ipc => CloneFlags::CLONE_NEWIPC,
			runtime::LinuxNamespaceType::mount => CloneFlags::CLONE_NEWNS,
			runtime::LinuxNamespaceType::network => CloneFlags::CLONE_NEWNET,
			runtime::LinuxNamespaceType::pid => CloneFlags::CLONE_NEWPID,
			runtime::LinuxNamespaceType::user => CloneFlags::CLONE_NEWUSER,
			runtime::LinuxNamespaceType::uts => CloneFlags::CLONE_NEWUTS,
		};
		result.insert(flag);
	}
	return result;
}
