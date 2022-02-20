use crate::flags;
use oci_spec::runtime;
use std::{fs::File, os::unix::prelude::AsRawFd};

struct ConfiguredNamespace<'a>(File, &'a runtime::LinuxNamespace);

pub fn join_namespaces(namespaces: &[runtime::LinuxNamespace]) {
	let mut configured_ns: Vec<ConfiguredNamespace> = Vec::new();
	for ns in namespaces {
		if let Some(path) = ns.path().as_ref() {
			configured_ns.push(ConfiguredNamespace(
				File::open(path).unwrap_or_else(|_| {
					panic!(
						"failed to open {:?} for NS {:?}",
						ns.path().as_ref().unwrap(),
						ns.typ()
					)
				}),
				ns,
			));
		} else {
			debug!(
				"Namespace {:?} has no path, skipping in join_namespaces",
				ns.typ()
			);
		}
	}

	for ns_config in &configured_ns {
		debug!("joining namespace {:?}", ns_config.1);
		let flags = flags::get_cloneflag(ns_config.1.typ());
		nix::sched::setns(ns_config.0.as_raw_fd(), flags)
			.unwrap_or_else(|_| panic!("Failed to join NS {:?}", ns_config.1));
	}
}
