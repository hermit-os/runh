use std::io::Write;
use std::{
	env,
	fs::File,
	os::unix::prelude::{FromRawFd, RawFd},
};

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
	std::thread::sleep(std::time::Duration::from_secs(10));
}
