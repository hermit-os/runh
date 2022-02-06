use cgroups_rs::cgroup_builder::CgroupBuilder;
use cgroups_rs::devices::*;
use cgroups_rs::Cgroup;
use std::fs::OpenOptions;
use std::io::prelude::*;

use crate::container::OCIContainer;

pub fn create_container(id: Option<&str>, bundle: Option<&str>, pidfile: Option<&str>) {
	let mut path = crate::get_project_dir();

	let _ = std::fs::create_dir(path.clone());

	path.push(id.unwrap());
	std::fs::create_dir(path.clone()).expect("Unable to create container directory");
	let container = OCIContainer::new(
		bundle.unwrap().to_string(),
		id.unwrap().to_string(),
		pidfile
			.unwrap_or(&(path.to_str().unwrap().to_owned() + "/containerpid"))
			.to_string(),
	);

	// write container to disk
	path.push("container.json");
	let mut file = OpenOptions::new()
		.read(true)
		.write(true)
		.create_new(true)
		.open(path)
		.expect("Unable to create container");
	file.write_all(serde_json::to_string(&container).unwrap().as_bytes())
		.unwrap();

	let h = cgroups_rs::hierarchies::auto();
	let _cgroup: Cgroup = CgroupBuilder::new(&("hermit_".to_owned() + id.unwrap()))
		.memory()
		.done()
		.cpu()
		.shares(100)
		.done()
		.devices()
		.device(
			-1,
			-1,
			DeviceType::All,
			false,
			vec![
				DevicePermissions::Read,
				DevicePermissions::Write,
				DevicePermissions::MkNod,
			],
		)
		.device(
			5,
			1,
			DeviceType::Char,
			true,
			vec![DevicePermissions::Read, DevicePermissions::Write],
		)
		.device(
			1,
			5,
			DeviceType::Char,
			true,
			vec![DevicePermissions::Read, DevicePermissions::Write],
		)
		.device(
			1,
			3,
			DeviceType::Char,
			true,
			vec![DevicePermissions::Read, DevicePermissions::Write],
		)
		.device(
			1,
			8,
			DeviceType::Char,
			true,
			vec![DevicePermissions::Read, DevicePermissions::Write],
		)
		.device(
			1,
			9,
			DeviceType::Char,
			true,
			vec![DevicePermissions::Read, DevicePermissions::Write],
		)
		.done()
		.network()
		.done()
		.blkio()
		.done()
		.build(h);

	debug!(
		"Create container with uid {}, gid {}",
		container.spec().process().as_ref().unwrap().user().uid(),
		container.spec().process().as_ref().unwrap().user().gid()
	);
}
