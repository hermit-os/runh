#[macro_use]
extern crate colour;
#[macro_use]
extern crate log;

mod container;
mod cri;
mod logging;
mod pull;
mod spec;

use crate::pull::pull_registry;
use crate::spec::create_spec;
use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg, SubCommand};
use std::env;

fn parse_matches(app: App) {
	let matches = app.get_matches();

	// initialize logger
	logging::init(matches.value_of("LOG_LEVEL"));
	debug!("Welcome to runh {}", crate_version!());

	if let Some(ref matches) = matches.subcommand_matches("spec") {
		if let Some(str) = matches.value_of("BUNDLE") {
			let path = std::path::PathBuf::from(str);
			create_spec(std::fs::canonicalize(path).expect("Unable to determin absolte path"));
		} else {
			let path = env::current_dir().expect("Unable to determine current working dirctory");
			create_spec(path);
		}
	} else if let Some(ref matches) = matches.subcommand_matches("pull") {
		if let Some(str) = matches.value_of("IMAGE") {
			pull_registry(
				str,
				matches.value_of("USERNAME"),
				matches.value_of("PASSWORD"),
				matches.value_of("BUNDLE"),
			);
		} else {
			error!("Image name is missing");
		}
	} else {
		error!(
			"Subcommand is missing or currently not supported! Run `runh -h` for more information!"
		);
	}
}
pub fn main() {
	let app = App::new("runh")
		.version(crate_version!())
		.setting(AppSettings::ColoredHelp)
		.author(crate_authors!("\n"))
		.about(crate_description!())
		.arg(
			Arg::with_name("LOG_LEVEL")
				.long("log-level")
				.short("l")
				.default_value("info")
				.possible_values(&["trace", "debug", "info", "warn", "error", "off"])
				.help("The logging level of the application."),
		)
		.subcommand(
			SubCommand::with_name("spec")
				.about("Create a new specification file")
				.version(crate_version!())
				.arg(
					Arg::with_name("BUNDLE")
						.long("bundle")
						.short("b")
						.takes_value(true)
						.help("path to the root of the bundle directory"),
				),
		)
		.subcommand(
			SubCommand::with_name("create")
				.about("Create a container")
				.version(crate_version!()),
		)
		.subcommand(
			SubCommand::with_name("run")
				.about("Create and run a container")
				.version(crate_version!()),
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
