use std::{
	fs::{File, OpenOptions},
	os::unix::prelude::{AsRawFd, OpenOptionsExt},
	path::PathBuf,
};

use nix::{
	fcntl::OFlag,
	sys::{socket::ControlMessage, socket::MsgFlags, stat::Mode, uio::IoVec},
};

use crate::mounts;

nix::ioctl_write_ptr_bad!(ioctl_set_winsize, libc::TIOCSWINSZ, libc::winsize);

nix::ioctl_write_int_bad!(ioctl_set_ctty, libc::TIOCSCTTY);

pub fn setup_console(console_socket: File, win_size: Option<&nix::pty::Winsize>) {
	// Open a new PTY master
	let master_fd = nix::pty::posix_openpt(OFlag::O_RDWR | OFlag::O_CLOEXEC)
		.expect("Could not open pty master!");

	// Allow a slave to be generated for it
	//nix::pty::grantpt(&master_fd).expect("Could not grantpt!");
	nix::pty::unlockpt(&master_fd).expect("Could not unlockpt!");

	// Get the name of the slave
	let slave_name = nix::pty::ptsname_r(&master_fd).expect("Could not get ptsname!");

	if let Some(winsize) = win_size {
		unsafe { ioctl_set_winsize(master_fd.as_raw_fd(), winsize as *const libc::winsize) }
			.expect("Could not set winsize using ioctl!");
	}

	mounts::mount_console(&PathBuf::from(&slave_name));

	//Send master fd over console_socket
	nix::sys::socket::sendmsg(
		console_socket.as_raw_fd(),
		&[IoVec::from_slice("/dev/ptmx".as_bytes())],
		&[ControlMessage::ScmRights(&[master_fd.as_raw_fd()])],
		MsgFlags::empty(),
		None,
	)
	.expect("Could not send message to console socket!");

	let slave_fd = nix::fcntl::open(
		std::path::Path::new(&slave_name),
		OFlag::O_RDWR,
		Mode::empty(),
	)
	.expect("Could not open pty slave path!");
	for i in 0..3 {
		nix::unistd::dup3(slave_fd, i, OFlag::empty())
			.expect(format!("Could not dup3 pty slave_fd onto fd {}", i).as_str());
	}

    unsafe { ioctl_set_ctty(0, 0) }
			.expect("Could not set ctty!");

	//master_fd auto-closes on drop
}
