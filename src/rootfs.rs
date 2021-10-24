use std::{
	fs::OpenOptions,
	os::unix::prelude::{AsRawFd, OpenOptionsExt},
	path::PathBuf,
};

use nix::mount::{MntFlags, MsFlags};
use oci_spec::runtime::Spec;
use path_clean::PathClean;

// This function should be equivalent to cyphar/filepath-securejoin/SecureJoinVFS
pub fn resolve_in_rootfs(destination_rel: &PathBuf, rootfs: &PathBuf) -> PathBuf {
	let mut unsafe_path = destination_rel.clone();
	let mut destination_resolved = PathBuf::new();
	let mut n = 0;

	while let Some(subpath) = unsafe_path.clone().iter().next() {
		if n > 255 {
			panic!(
				"Could not resolve mount path at {:?}! Too many symlinks!",
				destination_rel
			);
		}

		unsafe_path = unsafe_path.strip_prefix(subpath).unwrap().to_path_buf();

		let clean_subpath = PathBuf::from("/")
			.join(&destination_resolved)
			.join(subpath)
			.clean();
		if clean_subpath == PathBuf::from("/") {
			destination_resolved.clear();
			continue;
		}

		let full_path = rootfs
			.join(clean_subpath.strip_prefix("/").unwrap())
			.clean();
		if !full_path.exists() {
			destination_resolved.push(subpath);
			continue;
		}

		let metadata = full_path.symlink_metadata().expect(
			format!(
				"Could not get metadata for mount path component at {:?}!",
				full_path
			)
			.as_str(),
		);
		if !metadata.file_type().is_symlink() {
			destination_resolved.push(subpath);
			continue;
		}

		n = n + 1;

		let link = full_path.read_link().expect(
			format!(
				"Could not read symlink for mount path component at {:?}!",
				full_path
			)
			.as_str(),
		);

		if link.is_absolute() {
			destination_resolved.clear();
		}
		unsafe_path = link.join(unsafe_path);
	}
	rootfs
		.join(
			PathBuf::from("/")
				.join(destination_resolved)
				.clean()
				.strip_prefix("/")
				.unwrap(),
		)
		.clean()
}

pub fn mount_rootfs(spec: &Spec, rootfs_path: &PathBuf) {
	let mut mount_flags = MsFlags::empty();
	mount_flags.insert(MsFlags::MS_REC);
	mount_flags.insert(
		match spec
			.linux()
			.as_ref()
			.unwrap()
			.rootfs_propagation()
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
					.linux()
					.as_ref()
					.unwrap()
					.rootfs_propagation()
					.as_ref()
					.unwrap()
			),
			None => MsFlags::MS_SLAVE,
		},
	);

	nix::mount::mount::<str, str, str, str>(None, "/", None, mount_flags, None).expect(
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

	nix::mount::mount::<PathBuf, PathBuf, str, str>(
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
	nix::mount::mount::<str, str, str, str>(None, "/", None, flags, None)
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

	nix::mount::mount::<str, str, str, str>(None, ".", None, mount_flags, None)
		.expect("Could not change old_root propagation type!");

	nix::mount::umount2(".", MntFlags::MNT_DETACH).expect("Could not unmount cwd!");

	nix::unistd::chdir("/").expect("Could not chdir into new_root at /!");
}
