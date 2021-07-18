use nix::sched::CloneFlags;
use oci_spec::runtime;

pub fn generate_cloneflags(namespaces: &Vec<runtime::LinuxNamespace>) -> CloneFlags {
	let mut result = CloneFlags::empty();
	for ns in namespaces {
		result.insert(get_cloneflag(ns.typ));
	}
	return result;
}

pub fn get_cloneflag(typ: runtime::LinuxNamespaceType) -> CloneFlags {
	match typ {
		runtime::LinuxNamespaceType::cgroup => CloneFlags::CLONE_NEWCGROUP,
		runtime::LinuxNamespaceType::ipc => CloneFlags::CLONE_NEWIPC,
		runtime::LinuxNamespaceType::mount => CloneFlags::CLONE_NEWNS,
		runtime::LinuxNamespaceType::network => CloneFlags::CLONE_NEWNET,
		runtime::LinuxNamespaceType::pid => CloneFlags::CLONE_NEWPID,
		runtime::LinuxNamespaceType::user => CloneFlags::CLONE_NEWUSER,
		runtime::LinuxNamespaceType::uts => CloneFlags::CLONE_NEWUTS,
	}
}
