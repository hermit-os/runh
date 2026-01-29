use crate::rootfs;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub fn find_in_path(path_relative: &Path, rootfs: Option<&Path>) -> Option<PathBuf> {
	let mut path_relative = Cow::Borrowed(path_relative);
	if path_relative.is_absolute() {
		if let Some(rootfs_path) = rootfs {
			path_relative = Cow::Owned(rootfs::resolve_in_rootfs(&path_relative, rootfs_path));
		}
		if path_relative.exists() {
			Some(path_relative.into_owned())
		} else {
			None
		}
	} else {
		let path = std::env::var("PATH")
			.expect("PATH environment variable not set and no absolute args-path given!");
		for folder in path.split(':') {
			let try_abs_path = PathBuf::from(folder).join(&path_relative);
			let try_abs_path = match rootfs {
				None => try_abs_path,
				Some(rootfs_path) => rootfs::resolve_in_rootfs(&try_abs_path, rootfs_path),
			};
			if try_abs_path.exists() {
				return Some(try_abs_path);
			}
		}
		None
	}
}
