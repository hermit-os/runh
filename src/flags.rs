use std::ffi::c_int;

use nix::sched::CloneFlags;
use oci_spec::runtime;

pub fn generate_cloneflags(namespaces: &[runtime::LinuxNamespace]) -> CloneFlags {
	let mut result = CloneFlags::empty();
	for ns in namespaces {
		if ns.path().is_none() {
			result.insert(get_cloneflag(ns.typ()));
		}
	}
	result
}

pub fn get_cloneflag(typ: runtime::LinuxNamespaceType) -> CloneFlags {
	match typ {
		runtime::LinuxNamespaceType::Cgroup => CloneFlags::CLONE_NEWCGROUP,
		runtime::LinuxNamespaceType::Ipc => CloneFlags::CLONE_NEWIPC,
		runtime::LinuxNamespaceType::Mount => CloneFlags::CLONE_NEWNS,
		runtime::LinuxNamespaceType::Network => CloneFlags::CLONE_NEWNET,
		runtime::LinuxNamespaceType::Pid => CloneFlags::CLONE_NEWPID,
		runtime::LinuxNamespaceType::User => CloneFlags::CLONE_NEWUSER,
		runtime::LinuxNamespaceType::Uts => CloneFlags::CLONE_NEWUTS,
		runtime::LinuxNamespaceType::Time => {
			// TODO: This is missing from both libc and nix.
			// https://github.com/rust-lang/libc/issues/3033
			const CLONE_NEWTIME: c_int = 0x80;
			unsafe { CloneFlags::from_bits_unchecked(CLONE_NEWTIME) }
		}
	}
}
