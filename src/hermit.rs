use goblin::elf;
use goblin::elf64::header::EI_OSABI;
use std::{fs, path::PathBuf};

pub fn is_hermit_app(path: &PathBuf) -> bool {
	let buffer = fs::read(path)
		.expect(format!("Could not read content of args-executable at {:?}", path).as_str());
	if let Ok(elf) = elf::Elf::parse(&buffer) {
		return elf.header.e_ident[EI_OSABI] == 0xFF;
	} else {
		warn!("Could not parse content of args-executable in ELF format. Might be a script file. Assuming non-hermit container...");
		return false;
	}
}

pub fn create_environment(path: &PathBuf) {}

pub fn get_environment_path(project_dir: &PathBuf) -> PathBuf {
	PathBuf::from("/global/projects/runh/hermit")
}

pub fn prepare_environment(rootfs: &PathBuf, project_dir: &PathBuf) {
	let environment_path = get_environment_path(project_dir);
	if !environment_path.exists() {
		create_environment(&environment_path);
	} else if !environment_path.is_dir() {
		panic!(
			"Environment path at {:?} exists but is not a directory!",
			&environment_path
		);
	}

	let hermit_path = rootfs.join("hermit");
}

pub fn setup_environment(rootfs: &PathBuf) {}
