#[macro_use]
extern crate colour;
#[macro_use]
extern crate log;

mod container;
mod create;
mod delete;
//mod exec;
mod console;
mod consts;
mod devices;
mod flags;
mod init;
mod list;
mod logging;
mod mounts;
mod namespaces;
mod network;
mod paths;
mod pull;
mod rootfs;
mod spec;
mod start;

use crate::create::*;
use crate::delete::*;
//use crate::exec::*;
use crate::init::*;
use crate::list::*;
use crate::pull::*;
use crate::spec::*;
use crate::start::*;
use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg, SubCommand};
use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::{env, path::PathBuf};

fn parse_matches(app: App) {
	let matches = app.get_matches();

	let project_dir = PathBuf::from(matches.value_of("ROOT").unwrap());

	if !project_dir.exists() {
		DirBuilder::new()
			.recursive(true)
			.mode(0o755)
			.create(&project_dir)
			.expect(format!("Could not create root directory at {:?}", &project_dir).as_str());
	}

	// initialize logger
	logging::init(
		project_dir.clone(),
		matches.value_of("LOG_PATH"),
		matches.value_of("LOG_FORMAT"),
		matches.value_of("LOG_LEVEL"),
	);
	info!("Welcome to runh {}", crate_version!());
	debug!(
		"Runh was started with command {}",
		env::args().collect::<Vec<String>>().join(" ")
	);

	match matches.subcommand() {
		("spec", Some(sub_m)) => create_spec(sub_m.value_of("BUNDLE")),
		("create", Some(sub_m)) => create_container(
			project_dir,
			sub_m.value_of("CONTAINER_ID"),
			sub_m.value_of("BUNDLE"),
			sub_m.value_of("PID_FILE"),
			sub_m.value_of("CONSOLE_SOCKET"),
		),
		("delete", Some(sub_m)) => delete_container(project_dir, sub_m.value_of("CONTAINER_ID")),
		("start", Some(sub_m)) => start_container(project_dir, sub_m.value_of("CONTAINER_ID")),
		("init", Some(_)) => init_container(),
		("list", Some(_)) => list_containers(project_dir),
		("pull", Some(sub_m)) => {
			if let Some(str) = sub_m.value_of("IMAGE") {
				pull_registry(
					str,
					sub_m.value_of("USERNAME"),
					sub_m.value_of("PASSWORD"),
					sub_m.value_of("BUNDLE"),
				);
			} else {
				error!("Image name is missing");
			}
		}
		_ => {
			error!(
				"Subcommand is missing or currently not supported! Run `runh -h` for more information!"
			);
		}
	}
}
pub fn main() {
	std::panic::set_hook(Box::new(|panic_info| {
		error!("PANIC: {}", panic_info);
	}));

	let app = App::new("runh")
		.version(crate_version!())
		.setting(AppSettings::ColoredHelp)
		.author(crate_authors!("\n"))
		.about(crate_description!())
		.arg(
			Arg::with_name("ROOT")
				.long("root")
				.takes_value(true)
				.default_value("/run/user/1000/runh")
				.help("root directory for storage of vm state"),
		)
		.arg(
			Arg::with_name("LOG_LEVEL")
				.long("log-level")
				.short("l")
				.default_value("debug")
				.possible_values(&["trace", "debug", "info", "warn", "error", "off"])
				.help("The logging level of the application."),
		)
		.arg(
			Arg::with_name("LOG_PATH")
				.long("log")
				.takes_value(true)
				.help("set the log file path"),
		)
		.arg(
			Arg::with_name("LOG_FORMAT")
				.long("log-format")
				.default_value("text")
				.possible_values(&["text", "json"])
				.help("set the log format"),
		)
		.arg(
			Arg::with_name("SYSTEMD_CGROUP")
				.long("systemd-cgroup")
				.takes_value(false)
				.help("Currently unimplemented!")
		)
		.subcommand(
			SubCommand::with_name("spec")
				.about("Create a new specification file")
				.version(crate_version!())
				.arg(
					Arg::with_name("BUNDLE")
						.long("bundle")
						.short("b")
						.required(true)
						.takes_value(true)
						.help("path to the root of the bundle directory"),
				),
		)
		.subcommand(
			SubCommand::with_name("create")
				.about("Create a container")
				.version(crate_version!())
				.arg(
					Arg::with_name("CONTAINER_ID")
						.takes_value(true)
						.required(true)
						.help("Id of the container"),
				)
				.arg(
					Arg::with_name("BUNDLE")
						.long("bundle")
						.short("b")
						.takes_value(true)
						.required(true)
						.help("Path to the root of the bundle directory"),
				)
				.arg(
					Arg::with_name("PID_FILE")
						.long("pid-file")
						.takes_value(true)
						.required(false)
						.help("File to write the process id to"),
				)
				.arg(
					Arg::with_name("CONSOLE_SOCKET")
						.long("console-socket")
						.takes_value(true)
						.help("Path to an AF_UNIX socket for console IO")
				),
		)
		.subcommand(
			SubCommand::with_name("exec")
				.about("Execute new process inside the container")
				.version(crate_version!())
				.arg(
					Arg::with_name("CONTAINER_ID")
						.takes_value(true)
						.required(true)
						.help("Id of the container"),
				)
				.arg(
					Arg::with_name("COMMAND")
						.takes_value(true)
						.required(true)
						.help("Command, which will be executed in the container"),
				)
				.arg(
					Arg::with_name("COMMAND OPTIONS")
						.help("Arguments of the command")
						.required(false)
						.multiple(true)
						.max_values(255),
				),
		)
		.subcommand(
			SubCommand::with_name("delete")
				.about("Delete an existing container")
				.version(crate_version!())
				.arg(
					Arg::with_name("CONTAINER_ID")
						.takes_value(true)
						.required(true)
						.help("Id of the container"),
				)
				.arg(
					Arg::with_name("FORCE")
						.long("force")
						.short("f")
						.takes_value(false)
						.required(false)
						.help("Currently unimplemented!"),
				),
		)
		.subcommand(
			SubCommand::with_name("list")
				.about("Create and run a container")
				.version(crate_version!()),
		)
		.subcommand(
			SubCommand::with_name("start")
				.about("Executes the user defined process in a created container")
				.version(crate_version!())
				.arg(
					Arg::with_name("CONTAINER_ID")
						.takes_value(true)
						.required(true)
						.help("Id of the container"),
				),
		)
		.subcommand(
			SubCommand::with_name("init")
				.about("Init process running inside a newly created container. Do not use outside of runh!")
				.version(crate_version!())
		)
		.subcommand(
			SubCommand::with_name("pull")
				.about("Pull an image or a repository from a registry")
				.version(crate_version!())
				.arg(
					Arg::with_name("IMAGE")
						.value_name("NAME[:TAG|@DIGEST]")
						.takes_value(true)
						.help("image or a repository from a registry"),
				)
				.arg(
					Arg::with_name("USERNAME")
						.long("username")
						.short("u")
						.takes_value(true)
						.help("Username"),
				)
				.arg(
					Arg::with_name("PASSWORD")
						.long("password")
						.short("p")
						.takes_value(true)
						.help("Password"),
				)
				.arg(
					Arg::with_name("BUNDLE")
						.long("bundle")
						.short("b")
						.takes_value(true)
						.help("Path to the root of the bundle directory"),
				),
		)
		.subcommand(
			SubCommand::with_name("checkpoint")
				.about("Checkpoint a running container (not supported)")
				.version(crate_version!()),
		)
		.subcommand(
			SubCommand::with_name("restore")
				.about("Restore a container from a previous checkpoint (not supported)")
				.version(crate_version!()),
		);

	parse_matches(app.clone());
}
