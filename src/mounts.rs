use oci_spec::runtime;
use std::path::PathBuf;

pub fn configure_mounts(
	mounts: &Vec<runtime::Mount>,
	rootfs: PathBuf,
	mount_label: Option<String>,
) {
	for mount in mounts {
		let mount_destination = mount.destination.trim_start_matches("/");
		let destination = &rootfs.join(mount_destination.to_owned());

		let mut destination_resolved = PathBuf::new();

		// Verfify destination path lies within rootfs folder (no symlinks out of it)
		for subpath in destination.iter() {
			destination_resolved.push(subpath);
			if destination_resolved.exists() {
				destination_resolved = destination_resolved.canonicalize().expect(
					format!("Could not resolve mount path at {:?}", destination_resolved).as_str(),
				);
			}
		}
		if destination_resolved.starts_with(&rootfs) {
			debug!("Mounting {:?}", destination_resolved);
			match mount.typ.as_ref().and_then(|x| Some(x.as_str())) {
				Some("sysfs") | Some("proc") => todo!("Mount sysfs|proc"),
				Some("mqueue") => todo!("Mount mqueue"),
				Some("tmpfs") => todo!("Mount tmpfs"),
				Some("bind") => todo!("Mount bind"),
				Some("cgroup") => todo!("Mount cgroup"),
				_ => todo!("Mount default"),
			}
		} else {
			panic!(
				"Mount at {} cannot be mounted into rootfs!",
				mount.destination
			);
		}
	}
}
