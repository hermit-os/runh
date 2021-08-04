use std::path::PathBuf;

pub fn find_in_path(path_relative: PathBuf) -> Option<PathBuf> {
	if path_relative.is_absolute() {
		if path_relative.exists() {
			return Some(path_relative);
		} else {
			return None;
		}
	} else {
		let path = std::env::var("PATH")
			.expect("PATH environment variable not set and no absolute args-path given!");
		for folder in path.split(":") {
			let try_abs_path = PathBuf::from(folder).join(&path_relative);
			if try_abs_path.exists() {
				return Some(try_abs_path);
			}
		}
		return None;
	};
}
