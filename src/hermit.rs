use crate::network;
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

pub fn get_qemu_args(
	kernel: &str,
	app: &str,
	netconf: &Option<network::HermitNetworkConfig>,
	app_args: &Vec<String>,
	micro_vm: bool,
) -> Vec<String> {
	let mut exec_args: Vec<String> = vec![
		"qemu-system-x86_64",
		"-enable-kvm",
		"-display",
		"none",
		"-smp",
		"1",
		"-m",
		"1G",
		"-serial",
		"stdio",
		"-kernel",
		kernel,
		"-initrd",
		app,
		"-cpu",
		"host",
	]
	.iter()
	.map(|s| s.to_string())
	.collect();

	if micro_vm {
		exec_args.append(
			&mut vec![
				"-M",
				"microvm,x-option-roms=off,pit=off,pic=off,rtc=on,auto-kernel-cmdline=off",
				"-global",
				"virtio-mmio.force-legacy=off",
				"-nodefaults",
				"-no-user-config",
				"-device",
				"isa-debug-exit,iobase=0xf4,iosize=0x04",
			]
			.iter()
			.map(|s| s.to_string())
			.collect(),
		);
	}

	if let Some(network_config) = netconf.as_ref() {
		exec_args.push("-netdev".to_string());
		exec_args.push("tap,id=net0,ifname=tap100,script=no,downscript=no".to_string());
		exec_args.push("-device".to_string());
		exec_args.push(if micro_vm {
			format!(
				"virtio-net-device,netdev=net0,mac={}",
				network_config.mac
			)
		} else {
			format!(
				"virtio-net-pci,netdev=net0,disable-legacy=on,mac={}",
				network_config.mac
			)
		});
	}

	exec_args.push("-append".to_string());

	let mut args_string = "".to_string();

	if let Some(network_config) = netconf.as_ref() {
		args_string = format!(
			"-ip {} -gateway {} -mask {}",
			network_config.ip.to_string(),
			network_config.gateway.to_string(),
			network_config.mask.to_string()
		);
	}

	if let Some(application_args) = app_args.get(1..) {
		args_string = format!("-freq 1197 {} -- {}", args_string, application_args.join(" "));
	}
	exec_args.push(args_string);

	exec_args
}
