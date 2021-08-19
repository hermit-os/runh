use std::{convert::TryInto, path::PathBuf};

use nix::{
	sys::stat::{Mode, SFlag},
	unistd::{Gid, Uid},
};
use oci_spec::runtime;

use crate::{mounts, rootfs};

pub fn create_devices(spec_devices: &Option<Vec<runtime::LinuxDevice>>, rootfs: &PathBuf) {
	let mut default_devices = vec![
		runtime::LinuxDevice {
			path: "/dev/null".to_string(),
			file_mode: Some(0o666u32),
			typ: runtime::LinuxDeviceType::c,
			major: 1,
			minor: 3,
			uid: Some(0),
			gid: Some(0),
		},
		runtime::LinuxDevice {
			path: "/dev/zero".to_string(),
			file_mode: Some(0o666u32),
			typ: runtime::LinuxDeviceType::c,
			major: 1,
			minor: 5,
			uid: Some(0),
			gid: Some(0),
		},
		runtime::LinuxDevice {
			path: "/dev/full".to_string(),
			file_mode: Some(0o666u32),
			typ: runtime::LinuxDeviceType::c,
			major: 1,
			minor: 7,
			uid: Some(0),
			gid: Some(0),
		},
		runtime::LinuxDevice {
			path: "/dev/random".to_string(),
			file_mode: Some(0o666u32),
			typ: runtime::LinuxDeviceType::c,
			major: 1,
			minor: 8,
			uid: Some(0),
			gid: Some(0),
		},
		runtime::LinuxDevice {
			path: "/dev/urandom".to_string(),
			file_mode: Some(0o666u32),
			typ: runtime::LinuxDeviceType::c,
			major: 1,
			minor: 9,
			uid: Some(0),
			gid: Some(0),
		},
		runtime::LinuxDevice {
			path: "/dev/tty".to_string(),
			file_mode: Some(0o666u32),
			typ: runtime::LinuxDeviceType::c,
			major: 5,
			minor: 0,
			uid: Some(0),
			gid: Some(0),
		}, //TODO: Include /dev/console and /dev/ptmx
	];

	let all_devices = spec_devices
		.as_ref()
		.and_then(|spec_devices| {
			let mut all_devices = spec_devices.clone();
			all_devices.append(&mut default_devices);
			all_devices.sort_by_key(|f| f.path.clone());
			all_devices.dedup_by_key(|f| f.path.clone());
			Some(all_devices)
		})
		.unwrap_or(default_devices);

	for dev in all_devices {
		debug!("Creating device {}", dev.path.as_str());

		if PathBuf::from(dev.path.as_str()).starts_with("/dev/ptmx") {
			//TODO: setup /dev/ptmx
			continue;
		}
		let destination_resolved = rootfs::resolve_in_rootfs(dev.path.as_str(), rootfs);
		if !destination_resolved.starts_with(&rootfs) {
			panic!("Device at {} cannot be mounted into rootfs!", dev.path);
		}
		mounts::create_all_dirs(&PathBuf::from(
			&destination_resolved.parent().expect(
				format!(
					"Could create device at destination {:?} which has no parent dir!",
					destination_resolved
				)
				.as_str(),
			),
		));

		//Just assume we are not in a user namespace and that mknod will work. Error out if it does not (FIXME ?)
		let node_kind = match dev.typ {
			runtime::LinuxDeviceType::c => SFlag::S_IFCHR,
			runtime::LinuxDeviceType::b => SFlag::S_IFBLK,
			runtime::LinuxDeviceType::p => SFlag::S_IFIFO,
			runtime::LinuxDeviceType::u => SFlag::S_IFCHR,
		};
		let mode = Mode::from_bits(dev.file_mode.unwrap_or(0o666u32)).unwrap();
		let device =
			nix::sys::stat::makedev(dev.major.try_into().unwrap(), dev.minor.try_into().unwrap());
		nix::sys::stat::mknod(&destination_resolved, node_kind, mode, device)
			.expect(format!("Could not create device {}!", dev.path).as_str());
		nix::unistd::chown(
			&destination_resolved,
			dev.uid.and_then(|f| Some(Uid::from_raw(f))),
			dev.gid.and_then(|f| Some(Gid::from_raw(f))),
		)
		.expect(format!("Could not chown device {}!", dev.path).as_str());
	}
}
