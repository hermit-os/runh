use std::{
	fs::OpenOptions,
	os::unix::prelude::{AsRawFd, OpenOptionsExt},
	path::Path,
	path::PathBuf,
};

use nix::mount::{MntFlags, MsFlags};
use oci_spec::runtime::Spec;
use path_clean::PathClean;

// This function should be equivalent to cyphar/filepath-securejoin/SecureJoinVFS
pub fn resolve_in_rootfs(destination_rel: &Path, rootfs: &Path) -> PathBuf {
	let mut unsafe_path = destination_rel.to_path_buf();
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

		let metadata = full_path.symlink_metadata().unwrap_or_else(|_| {
			panic!(
				"Could not get metadata for mount path component at {:?}!",
				full_path
			)
		});
		if !metadata.file_type().is_symlink() {
			destination_resolved.push(subpath);
			continue;
		}

		n += 1;

		let link = full_path.read_link().unwrap_or_else(|_| {
			panic!(
				"Could not read symlink for mount path component at {:?}!",
				full_path
			)
		});

		if link.is_absolute() {
			destination_resolved.clear();
		}
		unsafe_path = link.join(unsafe_path).to_path_buf();
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

pub fn mount_rootfs(spec: &Spec, rootfs_path: &Path) {
	let mut mount_flags = MsFlags::empty();
	mount_flags.insert(MsFlags::MS_REC);
	mount_flags.insert(
		match spec
			.linux()
			.as_ref()
			.unwrap()
			.rootfs_propagation()
			.as_ref()
			.map(|x| x.as_str())
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

	nix::mount::mount::<str, str, str, str>(None, "/", None, mount_flags, None).unwrap_or_else(
		|_| {
			panic!(
				"Could not mount rootfs with given MsFlags {:?}",
				mount_flags
			)
		},
	);

	//TODO: Make parent mount private (?)
	let mut bind_mount_flags = MsFlags::empty();
	bind_mount_flags.insert(MsFlags::MS_BIND);
	bind_mount_flags.insert(MsFlags::MS_REC);

	debug!("Mounting rootfs at {rootfs_path:?}");

	nix::mount::mount::<Path, Path, str, str>(
		Some(rootfs_path),
		rootfs_path,
		Some("bind"),
		bind_mount_flags,
		None,
	)
	.unwrap_or_else(|_| panic!("Could not bind-mount rootfs at {:?}", &rootfs_path));
}

pub fn set_rootfs_read_only() {
	let mut flags = MsFlags::MS_BIND;
	flags.insert(MsFlags::MS_REMOUNT);
	flags.insert(MsFlags::MS_RDONLY);
	if nix::mount::mount::<str, str, str, str>(None, "/", None, flags, None).is_err() {
		let stat =
			nix::sys::statvfs::statvfs("/").expect("Could not stat / after read-only remount!");

		let mount_flags_new = MsFlags::from_bits(flags.bits() | stat.flags().bits())
			.expect("Could not combine old and new mount flags!");

		nix::mount::mount::<str, str, str, str>(None, "/", None, mount_flags_new, None)
			.expect("Could not change / mount type!");
	} //The first mount should not fail unless we are in a user namespace so technically the content of the if-block is unreachable.
}

pub fn pivot_root(rootfs: &Path) {
	let old_root = OpenOptions::new()
		.read(true)
		.write(false)
		.mode(0o0)
		.custom_flags(libc::O_DIRECTORY)
		.open("/")
		.expect("Could not open old root!");

	let new_root = OpenOptions::new()
		.read(true)
		.write(false)
		.mode(0o0)
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
