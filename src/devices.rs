use std::{
	convert::TryInto, fs::OpenOptions, os::unix::prelude::OpenOptionsExt, path::Path, path::PathBuf,
};

use nix::{
	mount::MsFlags,
	sys::stat::{Mode, SFlag},
	unistd::{Gid, Uid},
};
use oci_spec::runtime;

use crate::{mounts, rootfs};

pub fn create_devices(spec_devices: &Option<Vec<runtime::LinuxDevice>>, rootfs: &Path) {
	let mut default_devices = vec![
		runtime::LinuxDeviceBuilder::default()
			.path(PathBuf::from("/dev/null"))
			.file_mode(0o666u32)
			.typ(runtime::LinuxDeviceType::C)
			.major(1)
			.minor(3)
			.uid(0u32)
			.gid(0u32)
			.build()
			.unwrap(),
		runtime::LinuxDeviceBuilder::default()
			.path(PathBuf::from("/dev/zero"))
			.file_mode(0o666u32)
			.typ(runtime::LinuxDeviceType::C)
			.major(1)
			.minor(5)
			.uid(0u32)
			.gid(0u32)
			.build()
			.unwrap(),
		runtime::LinuxDeviceBuilder::default()
			.path(PathBuf::from("/dev/full"))
			.file_mode(0o666u32)
			.typ(runtime::LinuxDeviceType::C)
			.major(1)
			.minor(7)
			.uid(0u32)
			.gid(0u32)
			.build()
			.unwrap(),
		runtime::LinuxDeviceBuilder::default()
			.path(PathBuf::from("/dev/random"))
			.file_mode(0o666u32)
			.typ(runtime::LinuxDeviceType::C)
			.major(1)
			.minor(8)
			.uid(0u32)
			.gid(0u32)
			.build()
			.unwrap(),
		runtime::LinuxDeviceBuilder::default()
			.path(PathBuf::from("/dev/urandom"))
			.file_mode(0o666u32)
			.typ(runtime::LinuxDeviceType::C)
			.major(1)
			.minor(9)
			.uid(0u32)
			.gid(0u32)
			.build()
			.unwrap(),
		runtime::LinuxDeviceBuilder::default()
			.path(PathBuf::from("/dev/tty"))
			.file_mode(0o666u32)
			.typ(runtime::LinuxDeviceType::C)
			.major(5)
			.minor(0)
			.uid(0u32)
			.gid(0u32)
			.build()
			.unwrap(),
	];

	let all_devices = spec_devices
		.as_ref()
		.map(|spec_devices| {
			let mut all_devices = spec_devices.clone();
			all_devices.append(&mut default_devices);
			all_devices.sort_by_key(|f| f.path().clone());
			all_devices.dedup_by_key(|f| f.path().clone());
			all_devices
		})
		.unwrap_or(default_devices);

	for dev in all_devices {
		debug!("Creating device {:?}", dev.path());

		if dev.path().starts_with("/dev/ptmx") {
			continue;
		}
		let destination_resolved = rootfs::resolve_in_rootfs(dev.path(), rootfs);
		if !destination_resolved.starts_with(rootfs) {
			panic!("Device at {:?} cannot be mounted into rootfs!", dev.path());
		}
		mounts::create_all_dirs(&PathBuf::from(
			&destination_resolved.parent().unwrap_or_else(|| {
				panic!(
					"Could create device at destination {:?} which has no parent dir!",
					destination_resolved
				)
			}),
		));

		//Just assume we are not in a user namespace and that mknod will work. Error out if it does not (FIXME ?)
		let node_kind = match dev.typ() {
			runtime::LinuxDeviceType::C => SFlag::S_IFCHR,
			runtime::LinuxDeviceType::B => SFlag::S_IFBLK,
			runtime::LinuxDeviceType::P => SFlag::S_IFIFO,
			runtime::LinuxDeviceType::U => SFlag::S_IFCHR,
			runtime::LinuxDeviceType::A => unimplemented!("Device type A (All) not supported!"),
		};
		let mode = Mode::from_bits(dev.file_mode().unwrap_or(0o666u32)).unwrap();
		let device = nix::sys::stat::makedev(
			dev.major().try_into().unwrap(),
			dev.minor().try_into().unwrap(),
		);
		nix::sys::stat::mknod(&destination_resolved, node_kind, mode, device)
			.unwrap_or_else(|_| panic!("Could not create device {:?}!", dev.path()));
		nix::unistd::chown(
			&destination_resolved,
			dev.uid().map(Uid::from_raw),
			dev.gid().map(Gid::from_raw),
		)
		.unwrap_or_else(|_| panic!("Could not chown device {:?}!", dev.path()));
	}
}

pub fn setup_ptmx(rootfs: &Path) {
	let ptmx_path = rootfs.join("dev/ptmx");
	if ptmx_path.is_dir() {
		std::fs::remove_dir(&ptmx_path).expect("Could not remove existing /dev/ptmx dir!");
	} else if ptmx_path.exists() {
		std::fs::remove_file(&ptmx_path).expect("Could not remove existing /dev/ptmx file!");
	}
	nix::unistd::symlinkat("pts/ptmx", None, &ptmx_path)
		.expect("Could not symlink pts/ptmx to /dev/ptmx!");
}

fn verify_device(path: &Path, major_exp: u64, minor_exp: u64) {
	let stat = nix::sys::stat::stat(path)
		.unwrap_or_else(|_| panic!("Could not stat existing device at {:?}", path));
	let major = nix::sys::stat::major(stat.st_rdev);
	let minor = nix::sys::stat::minor(stat.st_rdev);

	if major != major_exp || minor != minor_exp {
		panic!("Found existing device at {:?} for which device ID does not match (Expected {},{}; found {},{})!",
			path,
			major_exp,
			minor_exp,
			major,
			minor,
		);
	}
}

pub fn create_tun(rootfs: &Path, uid: Uid, gid: Gid) {
	let destination_relative = PathBuf::from("/dev/net/tun");
	let destination_resolved = rootfs::resolve_in_rootfs(&destination_relative, rootfs);
	if !destination_resolved.starts_with(rootfs) {
		panic!(
			"Device at {:?} cannot be mounted into rootfs!",
			&destination_relative
		);
	}

	let parent = destination_resolved.parent().unwrap_or_else(|| {
		panic!(
			"Could create device at destination {:?} which has no parent dir!",
			destination_resolved
		)
	});
	if !parent.exists() {
		mounts::create_all_dirs(&PathBuf::from(&destination_resolved.parent().unwrap()));
	}

	if destination_resolved.exists() {
		verify_device(&destination_resolved, 10, 200);
		return;
	}

	//Just assume we are not in a user namespace and that mknod will work. Error out if it does not (FIXME ?)
	let node_kind = SFlag::S_IFCHR;
	let mode = Mode::from_bits(0o755u32).unwrap();
	let device = nix::sys::stat::makedev(10, 200);
	nix::sys::stat::mknod(&destination_resolved, node_kind, mode, device)
		.unwrap_or_else(|_| panic!("Could not create device {:?}!", &destination_relative));
	nix::unistd::chown(&destination_resolved, Some(uid), Some(gid))
		.unwrap_or_else(|_| panic!("Could not chown device {:?}!", &destination_relative));
}

pub fn mount_hermit_devices(rootfs: &Path) {
	if std::fs::metadata("/dev/kvm").is_ok() {
		mount_device(rootfs, &PathBuf::from("/dev/kvm"), 10, 232);
	} else {
		warn!("/dev/kvm doesn't exist and is consequently not supported!");
	}

	if std::fs::metadata("/dev/vhost-net").is_ok() {
		mount_device(rootfs, &PathBuf::from("/dev/vhost-net"), 10, 238);
	} else {
		warn!("/dev/vhost-net doesn't exist and is consequently not supported!");
	}
}

fn mount_device(rootfs: &Path, destination_rel: &Path, major: u64, minor: u64) {
	let destination = rootfs::resolve_in_rootfs(destination_rel, rootfs);
	let parent = destination.parent().unwrap_or_else(|| {
		panic!(
			"Could create device at destination {:?} which has no parent dir!",
			destination
		)
	});

	if !parent.exists() {
		mounts::create_all_dirs(&PathBuf::from(&destination.parent().unwrap()));
	}

	if destination.exists() {
		verify_device(&destination, major, minor);
		return;
	}
	if !destination.exists() {
		let _ = OpenOptions::new()
			.mode(0o755)
			.create(true)
			.write(true)
			.open(&destination)
			.unwrap_or_else(|_| {
				panic!(
					"Could not create destination for bind mount at {:?}",
					destination
				)
			});
	}

	mounts::mount_with_flags(
		"bind",
		destination_rel,
		destination_rel,
		&destination,
		mounts::MountOptions {
			mount_flags: MsFlags::MS_BIND,
			propagation_flags: MsFlags::empty(),
			data: None,
		},
		None,
	);
}

pub fn setup_dev_symlinks(rootfs: &Path) {
	// if PathBuf::from("/proc/kcore").exists() {
	// 	nix::unistd::symlinkat("/proc/kcore", None, &rootfs.join("dev/core"))
	// 		.expect("Could not symlink /proc/kcore to /dev/core");
	// }
	nix::unistd::symlinkat("/proc/self/fd", None, &rootfs.join("dev/fd"))
		.expect("Could not symlink /proc/self/fd to /dev/fd");
	nix::unistd::symlinkat("/proc/self/fd/0", None, &rootfs.join("dev/stdin"))
		.expect("Could not symlink /proc/self/fd/0 to /dev/stdin");
	nix::unistd::symlinkat("/proc/self/fd/1", None, &rootfs.join("dev/stdout"))
		.expect("Could not symlink /proc/self/fd/1 to /dev/stdout");
	nix::unistd::symlinkat("/proc/self/fd/2", None, &rootfs.join("dev/stderr"))
		.expect("Could not symlink /proc/self/fd/2 to /dev/stderr");
}
