use crate::container::OCIContainer;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono::TimeZone;
use std::ffi::CStr;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::{mem, ptr};

fn get_unix_username(uid: u32) -> Option<String> {
	unsafe {
		let mut result = ptr::null_mut();
		let amt = match libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) {
			n if n < 0 => 512 as usize,
			n => n as usize,
		};
		let mut buf = Vec::with_capacity(amt);
		let mut passwd: libc::passwd = mem::zeroed();

		match libc::getpwuid_r(
			uid,
			&mut passwd,
			buf.as_mut_ptr(),
			buf.capacity() as libc::size_t,
			&mut result,
		) {
			0 if !result.is_null() => {
				let ptr = passwd.pw_name as *const _;
				let username = CStr::from_ptr(ptr).to_str().unwrap().to_owned();
				Some(username)
			}
			_ => None,
		}
	}
}

pub fn list_containers(project_dir: PathBuf) {
	println!(
		"{0: <12} {1: <12} {2: <12} {3: <12} {4: <12} {5: <12}",
		"ID", "PID", "STATUS", "BUNDLE", "CREATED", "OWNER"
	);

	if project_dir.is_dir() {
		for entry in std::fs::read_dir(project_dir).unwrap() {
			let dir = entry.unwrap();
			let mut fname = dir.path().clone();
			fname.push("container.json");
			let mut uts = dir.path();
			uts.push("uts");

			if let Ok(mut file) = OpenOptions::new().read(true).write(false).open(fname) {
				let mut contents = String::new();
				file.read_to_string(&mut contents)
					.expect("Unable to read container spec");
				let metadata = file.metadata().expect("Unable to get file information");
				let created = if let Ok(systime) = metadata.created() {
					let datetime: DateTime<Utc> = DateTime::from(systime);
					datetime
				} else {
					let datetime: DateTime<Utc> = Utc.ymd(1970, 1, 1).and_hms(0, 0, 0);
					datetime
				};
				let user = get_unix_username(metadata.uid()).unwrap_or("".to_string());
				let status = if uts.exists() { "RUNNING" } else { "CREATED" };

				if let Ok(container) = serde_json::from_str::<OCIContainer>(&contents) {
					println!(
						"{0: <12} {1: <12} {2: <12} {3: <12} {4: <12} {5: <12}",
						dir.file_name().into_string().unwrap(),
						"",
						status,
						container.bundle(),
						created.format("%Y-%m-%d %H:%M:%S"),
						user
					);
				}
			}
		}
	}
}
