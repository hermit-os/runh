use crate::network;
use goblin::elf;
use goblin::elf64::header::EI_OSABI;
use std::{fs, path::Path, path::PathBuf};

pub fn is_hermit_app(path: &Path) -> bool {
	let buffer = fs::read(path)
		.unwrap_or_else(|_| panic!("Could not read content of args-executable at {:?}", path));
	if let Ok(elf) = elf::Elf::parse(&buffer) {
		elf.header.e_ident[EI_OSABI] == 0xFF
	} else {
		warn!("Could not parse content of args-executable in ELF format. Might be a script file. Assuming non-hermit container...");
		false
	}
}

pub fn create_environment(_path: &Path) {
	//TODO
}

pub fn get_environment_path(project_dir: &Path, hermit_env_path: &Option<&str>) -> PathBuf {
	match hermit_env_path {
		Some(s) => PathBuf::from(s),
		None => project_dir.join("hermit"),
	}
}

pub fn prepare_environment(project_dir: &Path, hermit_env_path: &Option<&str>) {
	let environment_path = get_environment_path(project_dir, hermit_env_path);
	if !environment_path.exists() {
		create_environment(&environment_path);
	} else if !environment_path.is_dir() {
		panic!(
			"Environment path at {:?} exists but is not a directory!",
			&environment_path
		);
	}
}

pub fn get_qemu_args(
	kernel: &str,
	app: &str,
	netconf: &Option<network::HermitNetworkConfig>,
	app_args: &[String],
	micro_vm: bool,
	kvm: bool,
	tap_fd: &Option<i32>,
) -> Vec<String> {
	let mut exec_args: Vec<String> = vec![
		"qemu-system-x86_64",
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
	]
	.iter()
	.map(|s| s.to_string())
	.collect();

	if kvm {
		exec_args.append(
			&mut vec!["--enable-kvm", "-cpu", "host"]
				.iter()
				.map(|s| s.to_string())
				.collect(),
		);
	} else {
		exec_args.append(
			&mut vec![
				"-cpu",
				"qemu64,apic,fsgsbase,rdtscp,xsave,xsaveopt,fxsr,rdrand",
			]
			.iter()
			.map(|s| s.to_string())
			.collect(),
		);
	}

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
		exec_args.push(format!("tap,id=net0,fd={}", tap_fd.unwrap()));
		exec_args.push("-device".to_string());
		exec_args.push(if micro_vm {
			format!("virtio-net-device,netdev=net0,mac={}", network_config.mac)
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
			network_config.ip, network_config.gateway, network_config.mask
		);
	}

	if let Some(application_args) = app_args.get(1..) {
		args_string = format!(
			"-freq 1197 {} -- {}",
			args_string,
			application_args.join(" ")
		);
	}
	exec_args.push(args_string);

	exec_args
}
