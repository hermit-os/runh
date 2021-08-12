use std::{
	fs::OpenOptions,
	os::unix::prelude::{AsRawFd, OpenOptionsExt},
	path::PathBuf,
};

use nix::mount::{MntFlags, MsFlags};
use oci_spec::runtime::Spec;

pub fn mount_rootfs(spec: &Spec, rootfs_path: &PathBuf) {
	let mut mount_flags = MsFlags::empty();
	mount_flags.insert(MsFlags::MS_REC);
	mount_flags.insert(
		match spec
			.linux
			.as_ref()
			.unwrap()
			.rootfs_propagation
			.as_ref()
			.and_then(|x| Some(x.as_str()))
		{
			Some("shared") => MsFlags::MS_SHARED,
			Some("slave") => MsFlags::MS_SLAVE,
			Some("private") => MsFlags::MS_PRIVATE,
			Some("unbindable") => MsFlags::MS_UNBINDABLE,
			Some(_) => panic!(
				"Value of rootfsPropagation did not match any known option! Given value: {}",
				&spec
					.linux
					.as_ref()
					.unwrap()
					.rootfs_propagation
					.as_ref()
					.unwrap()
			),
			None => MsFlags::MS_SLAVE,
		},
	);

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

	debug!("Mounting rootfs at {:?}", rootfs_path);

	nix::mount::mount::<PathBuf, PathBuf, str, Option<&str>>(
		Some(rootfs_path),
		rootfs_path,
		Some("bind"),
		bind_mount_flags,
		None,
	)
	.expect(format!("Could not bind-mount rootfs at {:?}", rootfs_path).as_str());
}

pub fn set_rootfs_read_only() {
	let mut flags = MsFlags::MS_BIND;
	flags.insert(MsFlags::MS_REMOUNT);
	flags.insert(MsFlags::MS_RDONLY);
	nix::mount::mount::<Option<&str>, str, Option<&str>, Option<&str>>(
		None, "/", None, flags, None,
	)
	.expect("Could not change / mount type!");
	//TODO: Mount again with flags |= statfs("/").flags
}

pub fn pivot_root(rootfs: &PathBuf) {
	let old_root = OpenOptions::new()
		.read(true)
		.write(false)
		.mode(0)
		.custom_flags(libc::O_DIRECTORY)
		.open("/")
		.expect("Could not open old root!");

	let new_root = OpenOptions::new()
		.read(true)
		.write(false)
		.mode(0)
		.custom_flags(libc::O_DIRECTORY)
		.open(rootfs)
		.expect("Could not open new root!");

	nix::unistd::fchdir(new_root.as_raw_fd()).expect("Could not fchdir into new root!");

	nix::unistd::pivot_root(".", ".").expect("Could not pivot root!");

	nix::unistd::fchdir(old_root.as_raw_fd()).expect("Could not fchdir to old root!");

	let mut mount_flags = MsFlags::MS_SLAVE;
	mount_flags.insert(MsFlags::MS_REC);

	nix::mount::mount::<Option<&str>, str, Option<&str>, Option<&str>>(
		None,
		".",
		None,
		mount_flags,
		None,
	)
	.expect("Could not change old_root propagation type!");

	nix::mount::umount2(".", MntFlags::MNT_DETACH).expect("Could not unmount cwd!");

	nix::unistd::chdir("/").expect("Could not chdir into new_root at /!");
}
