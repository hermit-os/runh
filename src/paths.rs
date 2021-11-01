use crate::rootfs;
use std::path::PathBuf;

pub fn find_in_path(mut path_relative: PathBuf, rootfs: Option<&PathBuf>) -> Option<PathBuf> {
	if path_relative.is_absolute() {
		if let Some(rootfs_path) = rootfs {
			path_relative = rootfs::resolve_in_rootfs(&path_relative, &rootfs_path);
		}
		if path_relative.exists() {
			return Some(path_relative);
		} else {
			return None;
		}
	} else {
		let path = std::env::var("PATH")
			.expect("PATH environment variable not set and no absolute args-path given!");
		for folder in path.split(":") {
			let try_abs_path = rootfs.map_or_else(
				|| PathBuf::from(folder).join(&path_relative),
				|rootfs_path| {
					rootfs::resolve_in_rootfs(
						&PathBuf::from(folder).join(&path_relative),
						&rootfs_path,
					)
				},
			);
			if try_abs_path.exists() {
				return Some(try_abs_path);
			}
		}
		return None;
	};
}
