use nix::{mount::MsFlags, sys::stat::Mode};
use oci_spec::runtime;
use std::{
	fs::{DirBuilder, File, OpenOptions},
	os::unix::{
		fs::DirBuilderExt,
		prelude::{AsRawFd, OpenOptionsExt},
	},
	path::Path,
	path::PathBuf,
};

use crate::rootfs;

#[derive(Clone)]
pub struct MountOptions {
	pub mount_flags: MsFlags,
	pub propagation_flags: MsFlags,
	pub data: Option<String>,
}

impl Default for MountOptions {
	fn default() -> Self {
		Self {
			mount_flags: MsFlags::empty(),
			propagation_flags: MsFlags::empty(),
			data: None,
		}
	}
}

pub fn mount_console(slave_path: &Path) {
	let old_umask = nix::sys::stat::umask(Mode::empty());
	let _ = OpenOptions::new()
		.mode(0o666)
		.create(true)
		.truncate(true)
		.write(true)
		.read(true)
		.open("/dev/console")
		.expect("Could not create /dev/console");

	nix::mount::mount::<Path, str, str, str>(
		Some(slave_path),
		"/dev/console",
		Some("bind"),
		MsFlags::MS_BIND,
		None,
	)
	.expect("Could not mount console at /dev/console!");

	let _ = nix::sys::stat::umask(old_umask);
}

pub fn configure_mounts(
	mounts: &[runtime::Mount],
	rootfs: &Path,
	bundle_rootfs: &Path,
	mount_label: &Option<String>,
) -> bool {
	let mut setup_dev = true;

	for mount in mounts {
		//Resolve mount source
		let mut mount_src = PathBuf::from(&mount.source().as_ref().unwrap());
		if !mount_src.is_absolute() {
			mount_src = bundle_rootfs.join(mount_src);
		}

		let mount_dest = PathBuf::from(&mount.destination());
		let mount_device = mount.typ().as_ref().unwrap().as_str();

		let mount_options = mount
			.options()
			.as_ref()
			.map(|options| parse_mount_options(options))
			.unwrap_or_default();

		let destination_resolved = rootfs::resolve_in_rootfs(mount.destination(), rootfs);

		if destination_resolved.starts_with(rootfs) {
			debug!(
				"Mounting {:?} with type {} and options {:?}",
				destination_resolved,
				mount.typ().as_ref().unwrap_or(&"none".to_string()),
				mount.options().as_ref().unwrap_or(&vec![])
			);

			let is_bind_mount = mount
				.options()
				.as_ref()
				.map(|options| {
					options.contains(&"bind".to_string()) || options.contains(&"rbind".to_string())
				})
				.unwrap_or(false);
			if is_bind_mount {
				if destination_resolved == PathBuf::from(&rootfs).join("dev") {
					setup_dev = false;
				}

				if !mount_src.exists() {
					panic!(
						"Tried to bind-mount source {:?} which does not exist!",
						mount_src
					);
				}
				if destination_resolved.starts_with(rootfs.join("proc")) {
					panic!(
						"Tried to mount source {:?} at destination {:?} which is in /proc",
						mount_src, mount_dest
					);
				} else {
					if mount_src.is_dir() {
						create_all_dirs(&destination_resolved);
					} else {
						create_all_dirs(&PathBuf::from(&destination_resolved.parent().unwrap_or_else(||
							panic!("Could not mount to destination {:?} which is not a directory and has no parent dir!", destination_resolved)
						)));
						if !destination_resolved.exists() {
							let _ = OpenOptions::new()
								.mode(0o755)
								.create(true)
								.truncate(true)
								.write(true)
								.open(&destination_resolved)
								.unwrap_or_else(|_| {
									panic!(
										"Could not create destination for bind mount at {:?}",
										destination_resolved
									)
								});
						}
					}

					mount_with_flags(
						"bind",
						&mount_src,
						&mount_dest,
						&destination_resolved,
						mount_options.clone(),
						mount_label.as_ref(),
					);

					let mut mount_options_copy = mount_options.clone();

					mount_options_copy.mount_flags.remove(MsFlags::MS_REC);
					mount_options_copy.mount_flags.remove(MsFlags::MS_REMOUNT);
					mount_options_copy.mount_flags.remove(MsFlags::MS_BIND);

					if !mount_options_copy.mount_flags.is_empty() {
						remount(
							"bind",
							&mount_src,
							&mount_dest,
							&destination_resolved,
							mount_options,
						);
					}
					//TODO: Relabel source (?)
				}
			} else {
				match mount.typ().as_ref().map(|x| x.as_str()) {
					Some("sysfs") | Some("proc") => {
						if !destination_resolved.exists() || destination_resolved.is_dir() {
							create_all_dirs(&destination_resolved);
							mount_with_flags(
								mount_device,
								&mount_src,
								&mount_dest,
								&destination_resolved,
								mount_options,
								None,
							);
						} else {
							panic!("Could not mount {:?}! sysfs and proc filesystems can only be mounted on directories!", destination_resolved);
						}
					}
					Some("mqueue") => {
						if !destination_resolved.exists() {
							create_all_dirs(&destination_resolved);
						}
						mount_with_flags(
							mount_device,
							&mount_src,
							&mount_dest,
							&destination_resolved,
							mount_options,
							None,
						);
					}
					Some("tmpfs") => {
						let tmpfs_mode = if !destination_resolved.exists() {
							create_all_dirs(&destination_resolved);
							None
						} else {
							Some(
								destination_resolved
									.metadata()
									.unwrap_or_else(|_| {
										panic!(
											"Could not read metadata for (existing) mount destination at {:?}",
											destination_resolved
										)
									})
									.permissions(),
							)
						};
						let is_read_only = mount_options.mount_flags.contains(MsFlags::MS_RDONLY);
						mount_with_flags(
							mount_device,
							&mount_src,
							&mount_dest,
							&destination_resolved,
							mount_options.clone(),
							mount_label.as_ref(),
						);

						if let Some(mode) = tmpfs_mode {
							std::fs::set_permissions(&destination_resolved, mode).unwrap_or_else(
								|_| {
									panic!(
									"Could not change permission on newly mounted tmpfs at {:?}",
									destination_resolved
								)
								},
							);
						}

						if is_read_only {
							remount(
								mount_device,
								&mount_src,
								&mount_dest,
								&destination_resolved,
								mount_options,
							);
						}
					}
					Some("cgroup") => {
						//TODO: Additional checks for cGroup v1 vs v2,
						//		mount might fail when the cgroup-NS was not unshared earlier
						// create_all_dirs(&destination_resolved);
						// mount_with_flags(
						// 	"cgroup2",
						// 	&mount_src,
						// 	&mount_dest,
						// 	&destination_resolved,
						// 	mount_options.clone(),
						// 	mount_label.as_ref(),
						// );
						warn!("Warning: cgroups are currently unimplemented!");
					}
					_ => {
						if destination_resolved.starts_with(rootfs.join("proc")) {
							panic!(
								"Tried to mount source {:?} at destination {:?} which is in /proc",
								mount_src, mount_dest
							);
						} else {
							create_all_dirs(&destination_resolved);
							mount_with_flags(
								mount_device,
								&mount_src,
								&mount_dest,
								&destination_resolved,
								mount_options.clone(),
								mount_label.as_ref(),
							);
						}
					}
				}
			}
		} else {
			panic!(
				"Mount at {:?} cannot be mounted into rootfs!",
				mount.destination()
			);
		}
	}
	setup_dev
}

fn remount(
	device: &str,
	mount_src: &Path,
	mount_dest: &Path,
	full_dest: &Path,
	mut options: MountOptions,
) {
	let procfd = open_trough_procfd(device, mount_dest, full_dest, &mut options);
	let procfd_path = PathBuf::from("/proc/self/fd").join(procfd.as_raw_fd().to_string());

	options.mount_flags.insert(MsFlags::MS_REMOUNT);
	nix::mount::mount::<Path, Path, str, str>(
		Some(mount_src),
		&procfd_path,
		Some(device.to_owned().as_str()),
		options.mount_flags,
		None,
	)
	.unwrap_or_else(|_| {
		panic!(
			"Could not remount source {:?} at destination path {:?}",
			mount_src, full_dest
		)
	});
}

pub fn mount_with_flags(
	device: &str,
	mount_src: &Path,
	mount_dest: &Path,
	full_dest: &Path,
	mut options: MountOptions,
	_label: Option<&String>,
) {
	// TODO: Format mount label with data string
	let procfd = open_trough_procfd(device, mount_dest, full_dest, &mut options);
	let procfd_path = PathBuf::from("/proc/self/fd").join(procfd.as_raw_fd().to_string());

	nix::mount::mount::<Path, Path, str, str>(
		Some(mount_src),
		&procfd_path,
		Some(device.to_owned().as_str()),
		options.mount_flags,
		options.data.as_deref(),
	)
	.unwrap_or_else(|_| {
		panic!(
			"Could not mount source {:?} at destination path {:?}",
			mount_src, full_dest
		)
	});

	if !options.propagation_flags.is_empty() {
		let new_procfd = open_trough_procfd(device, mount_dest, full_dest, &mut options);
		let new_procfd_path =
			PathBuf::from("/proc/self/fd").join(new_procfd.as_raw_fd().to_string());
		nix::mount::mount::<PathBuf, PathBuf, str, str>(
			None,
			&new_procfd_path,
			None,
			options.propagation_flags,
			None,
		)
		.unwrap_or_else(|_| {
			panic!(
				"Could not apply mount propagation for destination path {:?}",
				full_dest
			)
		});
	}
}

fn open_trough_procfd(
	device: &str,
	mount_dest: &Path,
	full_dest: &Path,
	options: &mut MountOptions,
) -> File {
	if mount_dest.to_path_buf() == PathBuf::from("/dev") || device == "tmpfs" {
		options.mount_flags.remove(MsFlags::MS_RDONLY);
	}

	let dest_file = OpenOptions::new()
		.custom_flags(libc::O_PATH | libc::O_CLOEXEC)
		.read(true)
		.write(false)
		.mode(0o0)
		.open(full_dest)
		.unwrap_or_else(|_| panic!("Could not open mount directory at {:?}!", full_dest));

	let mut procfd_path = PathBuf::new();
	procfd_path.push("/proc/self/fd");
	procfd_path.push(dest_file.as_raw_fd().to_string());

	let real_path = std::fs::read_link(&procfd_path).unwrap_or_else(|_| {
		panic!(
			"Could not read mount path at {:?} through proc fd!",
			full_dest
		)
	});
	if real_path != *full_dest {
		panic!(
			"procfd path and destination path do not equal for mount destination {:?}! procfd path was {:?}!",
			full_dest,
			real_path
		);
	}

	dest_file
}

pub fn create_all_dirs(dest: &Path) {
	DirBuilder::new()
		.recursive(true)
		.mode(0o755)
		.create(dest)
		.unwrap_or_else(|_| panic!("Could not create directories for {:?}", dest));
}

fn parse_mount_options(options: &[String]) -> MountOptions {
	let mut mount_flags = MsFlags::empty();
	let mut propagation_flags = MsFlags::empty();
	let mut data: Vec<String> = Vec::new();

	for option in options {
		match option.as_str() {
			"acl" => mount_flags.insert(MsFlags::MS_POSIXACL),
			"async" => mount_flags.remove(MsFlags::MS_SYNCHRONOUS),
			"atime" => mount_flags.remove(MsFlags::MS_NOATIME),
			"bind" => mount_flags.insert(MsFlags::MS_BIND),
			"defaults" => (),
			"dev" => mount_flags.remove(MsFlags::MS_NODEV),
			"diratime" => mount_flags.remove(MsFlags::MS_NODIRATIME),
			"dirsync" => mount_flags.insert(MsFlags::MS_DIRSYNC),
			"exec" => mount_flags.remove(MsFlags::MS_NOEXEC),
			"iversion" => mount_flags.insert(MsFlags::MS_I_VERSION),
			"lazytime" => unimplemented!("lazytime mount flag currently unsupported!"),
			"loud" => mount_flags.remove(MsFlags::MS_SILENT),
			"mand" => mount_flags.insert(MsFlags::MS_MANDLOCK),
			"noacl" => mount_flags.remove(MsFlags::MS_POSIXACL),
			"noatime" => mount_flags.insert(MsFlags::MS_NOATIME),
			"nodev" => mount_flags.insert(MsFlags::MS_NODEV),
			"nodiratime" => mount_flags.insert(MsFlags::MS_NODIRATIME),
			"noexec" => mount_flags.insert(MsFlags::MS_NOEXEC),
			"noiversion" => mount_flags.remove(MsFlags::MS_I_VERSION),
			"nolazytime" => unimplemented!("nolazytime mount flag currently unsupported!"),
			"nomand" => mount_flags.remove(MsFlags::MS_MANDLOCK),
			"norelatime" => mount_flags.remove(MsFlags::MS_RELATIME),
			"nostrictatime" => mount_flags.remove(MsFlags::MS_STRICTATIME),
			"nosuid" => mount_flags.insert(MsFlags::MS_NOSUID),
			"rbind" => {
				mount_flags.insert(MsFlags::MS_BIND);
				mount_flags.insert(MsFlags::MS_REC);
			}
			"relatime" => mount_flags.insert(MsFlags::MS_RELATIME),
			"remount" => mount_flags.insert(MsFlags::MS_REMOUNT),
			"ro" => mount_flags.insert(MsFlags::MS_RDONLY),
			"rw" => mount_flags.remove(MsFlags::MS_RDONLY),
			"silent" => mount_flags.insert(MsFlags::MS_SILENT),
			"strictatime" => mount_flags.insert(MsFlags::MS_STRICTATIME),
			"suid" => mount_flags.remove(MsFlags::MS_NOSUID),
			"sync" => mount_flags.insert(MsFlags::MS_SYNCHRONOUS),
			"private" => propagation_flags.insert(MsFlags::MS_PRIVATE),
			"shared" => propagation_flags.insert(MsFlags::MS_SHARED),
			"slave" => propagation_flags.insert(MsFlags::MS_SLAVE),
			"unbindable" => propagation_flags.insert(MsFlags::MS_UNBINDABLE),
			"rprivate" => {
				propagation_flags.insert(MsFlags::MS_PRIVATE);
				propagation_flags.insert(MsFlags::MS_REC)
			}
			"rshared" => {
				propagation_flags.insert(MsFlags::MS_SHARED);
				propagation_flags.insert(MsFlags::MS_REC)
			}
			"rslave" => {
				propagation_flags.insert(MsFlags::MS_SLAVE);
				propagation_flags.insert(MsFlags::MS_REC)
			}
			"runbindable" => {
				propagation_flags.insert(MsFlags::MS_UNBINDABLE);
				propagation_flags.insert(MsFlags::MS_REC)
			}
			"tmpcopyup" => unimplemented!("tmpcopyup mount flag currently unsupported!"),
			_ => {
				debug!("Mount option {option} not recognized, adding it to mount data string");
				data.push(option.to_owned());
			}
		}
	}

	MountOptions {
		mount_flags,
		propagation_flags,
		data: Some(data.join(",")),
	}
}
