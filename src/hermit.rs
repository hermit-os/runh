use crate::network;
use goblin::elf;
use goblin::elf64::header::EI_OSABI;
use std::{fs, path::Path};

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

#[derive(Debug)]
pub enum NetworkConfig {
	TapNetwork(network::VirtioNetworkConfig),
	UserNetwork(u16),
	None,
}

pub fn get_qemu_args(
	kernel: &str,
	app: &str,
	netconf: &NetworkConfig,
	app_args: &[String],
	micro_vm: bool,
	kvm_support: bool,
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
		"-device",
		"isa-debug-exit,iobase=0xf4,iosize=0x04",
		"-kernel",
		kernel,
		"-initrd",
		app,
	]
	.iter()
	.map(|s| s.to_string())
	.collect();

	if let Some(kvm) = crate::CONFIG.kvm {
		if kvm && kvm_support {
			exec_args.append(
				&mut ["--enable-kvm", "-cpu", "host"]
					.iter()
					.map(|s| s.to_string())
					.collect(),
			);
		} else {
			exec_args.append(
				&mut [
					"-cpu",
					"qemu64,apic,fsgsbase,rdtscp,xsave,xsaveopt,fxsr,rdrand",
				]
				.iter()
				.map(|s| s.to_string())
				.collect(),
			);
		}
	} else {
		// disable kvm support, if the configuration file doesn't enable it
		exec_args.append(
			&mut [
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
			&mut [
				"-M",
				"microvm,x-option-roms=off,pit=off,pic=off,rtc=on,auto-kernel-cmdline=off,acpi=off",
				"-global",
				"virtio-mmio.force-legacy=off",
				"-nodefaults",
				"-no-user-config",
			]
			.iter()
			.map(|s| s.to_string())
			.collect(),
		);
	} else {
		exec_args.append(
			&mut [
				"-chardev",
				"socket,id=char0,path=/run/vhostqemu",
				"-device",
				"vhost-user-fs-pci,queue-size=1024,chardev=char0,tag=root",
				"-object",
				"memory-backend-file,id=mem,size=1G,mem-path=/dev/shm,share=on",
				"-numa",
				"node,memdev=mem",
			]
			.iter()
			.map(|s| s.to_string())
			.collect(),
		);
	}

	let mut args_string = match netconf {
		NetworkConfig::TapNetwork(network_config) => {
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
			exec_args.push("-append".to_string());

			let args_string = format!(
				"-ip {} -gateway {} -mask {}",
				network_config.ip, network_config.gateway, network_config.mask
			);

			args_string
		}
		NetworkConfig::UserNetwork(user_port) => {
			exec_args.push("-netdev".to_string());
			exec_args.push(format!(
				"user,id=u1,hostfwd=tcp::{user_port}-:{user_port},net=192.168.76.0/24,dhcpstart=192.168.76.9"
			));
			exec_args.push("-device".to_string());
			exec_args.push("virtio-net-pci,netdev=u1,disable-legacy=on".to_string());
			exec_args.push("-append".to_string());

			"".to_string()
		}
		NetworkConfig::None => {
			exec_args.push("-append".to_string());
			"".to_string()
		}
	};

	if let Some(application_args) = app_args.get(1..) {
		args_string = format!("{} -- {}", args_string, application_args.join(" "));
	}
	exec_args.push(args_string);

	exec_args
}
