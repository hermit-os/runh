use std::io::Write;
use std::{
	env,
	fs::File,
	os::unix::prelude::{FromRawFd, RawFd},
};

use capctl::prctl;
use nix::sched;
use nix::sys::socket;

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
#[derive(Clone, Copy, Debug)]
struct SetupArgs {
	stage: InitStage,
	init_pipe: RawFd,
	parent_child_sync: SocketPair,
	parent_grandchild_sync: SocketPair,
}

#[repr(align(16))]
struct CloneArgs<'a> {
	stack: [u8; 4096],
	args: &'a SetupArgs,
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
		parent_grandchild_sync
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
			debug!("enter setup_parent");
			let child_pid = clone_process(CloneArgs {
				stack: [0; 4096],
				args: &SetupArgs {
					stage: InitStage::CHILD,
					init_pipe: args.init_pipe,
					parent_child_sync: args.parent_child_sync,
					parent_grandchild_sync: args.parent_grandchild_sync,
				},
				child_func: Box::new(init_stage),
			});
			debug!("Created child with pid {}", child_pid);

		}
		InitStage::CHILD => {
		}
		InitStage::GRANDCHILD => {}
	};
	return 0;
}
