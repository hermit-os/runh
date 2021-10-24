use nix::sched::CloneFlags;
use oci_spec::runtime;

pub fn generate_cloneflags(namespaces: &Vec<runtime::LinuxNamespace>) -> CloneFlags {
	let mut result = CloneFlags::empty();
	for ns in namespaces {
		if ns.path().is_none() {
			result.insert(get_cloneflag(ns.typ()));
		}
	}
	return result;
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
	}
}
