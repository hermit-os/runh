#[macro_use]
extern crate colour;
#[macro_use]
extern crate log;

mod container;
mod cri;
mod logging;
mod spec;

use crate::spec::create_spec;
use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg, SubCommand};
use std::env;

pub fn main() {
	let matches = App::new("runh")
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
		.get_matches();

	// initialize logger
	logging::init(matches.value_of("LOG_LEVEL"));
	info!("Welcome to runh {}", crate_version!());

	if let Some(ref matches) = matches.subcommand_matches("spec") {
		if let Some(str) = matches.value_of("BUNDLE") {
			let path = std::path::PathBuf::from(str.to_owned() + "config.json");
			create_spec(path).expect("Unable to create new specification file");
		} else {
			let mut path =
				env::current_dir().expect("Unable to determine current working dirctory");
			path.push("config.json");
			create_spec(path).expect("Unable to create new specification file");
		}
	}
}
