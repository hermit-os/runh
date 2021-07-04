use std::io::{Read, Write};
use std::os::unix::prelude::AsRawFd;
use std::{
	env,
	fs::File,
	os::unix::prelude::{FromRawFd, RawFd},
};

use capctl::prctl;
use nix::sched::{self, CloneFlags};
use nix::sys::socket;
use oci_spec::runtime;
use serde::Deserialize;
use serde::Serialize;

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
#[derive(Serialize, Deserialize, Debug)]
struct InitConfig {
	namespaces: Option<Vec<runtime::LinuxNamespace>>,
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

	debug!("read config from init_pipe");
	//FIXME: Actually read the config from the pipe

	let config = InitConfig { namespaces: None };

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
		init_pipe: pipe_fd,
		parent_child_sync,
		parent_grandchild_sync,
		config: &config,
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
			loop {}
		}
		InitStage::CHILD => {
			debug!("enter init_stage child");
			let _ = prctl::set_name("runh:CHILD");
			if let Some(namespaces) = &args.config.namespaces {
				join_namespaces(namespaces)
			}
		}
		InitStage::GRANDCHILD => {}
	};
	return 0;
}

struct ConfiguredNamespace<'a>(File, &'a runtime::LinuxNamespace);

fn join_namespaces(namespaces: &Vec<runtime::LinuxNamespace>) {
	let mut configured_ns: Vec<ConfiguredNamespace> = Vec::new();
	for ns in namespaces {
		configured_ns.push(ConfiguredNamespace(
			File::open(
				ns.path
					.as_ref()
					.expect(format!("namespace {} has no path!", ns.typ).as_str()),
			)
			.expect(
				format!(
					"failed to open {} for NS {}",
					ns.path.as_ref().unwrap(),
					ns.typ
				)
				.as_str(),
			),
			ns,
		));
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
