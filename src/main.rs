#[macro_use]
extern crate colour;
#[macro_use]
extern crate log;

mod container;
mod create;
mod delete;
mod kill;
//mod exec;
mod console;
mod consts;
mod devices;
mod flags;
mod hermit;
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
mod state;

use crate::create::*;
use crate::delete::*;
//use crate::exec::*;
use crate::init::*;
use crate::kill::*;
use crate::list::*;
use crate::logging::*;
use crate::pull::*;
use crate::spec::*;
use crate::start::*;
use crate::state::*;
use clap::{crate_version, Parser, Subcommand};
use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::{env, path::PathBuf};

fn parse_matches(cli: &Cli) {
	let project_dir = &cli.root;

	if !project_dir.exists() {
		DirBuilder::new()
			.recursive(true)
			.mode(0o755)
			.create(project_dir)
			.unwrap_or_else(|_| panic!("Could not create root directory at {:?}", &project_dir));
	}

	if let Commands::State { container_id } = &cli.command {
		logging::init(
			project_dir.clone(),
			cli.log.clone(),
			cli.log_format,
			cli.log_level,
			cli.debug_log,
		);

		print_container_state(project_dir.clone(), container_id);
		return;
	}

	// initialize logger
	logging::init(
		project_dir.clone(),
		cli.log.clone(),
		cli.log_format,
		cli.log_level,
		cli.debug_log,
	);
	info!("Welcome to runh {}", crate_version!());
	debug!(
		"Runh was started with command {}",
		env::args().collect::<Vec<String>>().join(" ")
	);

	match &cli.command {
		Commands::Spec { bundle, args } => create_spec(bundle.clone(), args.clone()),
		Commands::Create {
			container_id,
			bundle,
			pid_file,
			console_socket,
		} => create_container(
			project_dir.clone(),
			container_id,
			bundle.clone(),
			pid_file.clone(),
			console_socket.clone(),
			cli.hermit_env.clone(),
			cli.debug_config,
			cli.log_level,
		),
		Commands::Delete {
			container_id,
			force,
		} => delete_container(project_dir.clone(), container_id, *force),
		Commands::Kill {
			container_id,
			signal,
			all,
		} => kill_container(project_dir.clone(), container_id, signal, *all),
		Commands::Start { container_id } => start_container(project_dir.clone(), container_id),
		Commands::List => list_containers(project_dir.clone()),
		Commands::Init => init_container(),
		Commands::Pull {
			image,
			bundle,
			username,
			password,
		} => pull_registry(image, username, password, bundle.clone()),
		_ => {
			error!(
				"Subcommand is missing or currently not supported! Run `runh -h` for more information!"
			);
		}
	}
}

#[derive(Subcommand, Debug)]
#[command(author, version, about, long_about = None)]
#[command(next_line_help = true)]
enum Commands {
	/// Create a new specification file
	Spec {
		/// path to the root of the bundle directory
		#[arg(short = 'b', long)]
		bundle: PathBuf,

		/// container arguments
		#[arg(short = 'a', long)]
		args: Vec<String>,
	},
	/// Query container state
	State {
		/// Id of the container
		container_id: String,
	},
	/// Create a container
	Create {
		/// Id of the container
		container_id: String,
		/// path to the root of the bundle directory
		#[arg(short = 'b', long)]
		bundle: PathBuf,
		/// File to write the process id to
		#[arg(long)]
		pid_file: Option<PathBuf>,
		/// Path to an AF_UNIX socket for console IO
		#[arg(long)]
		console_socket: Option<PathBuf>,
	},
	/// Delete an existing container
	Delete {
		/// Id of the container
		container_id: String,

		/// Delete the container, even if it is still running
		#[arg(short = 'f', long, default_value_t)]
		force: bool,
	},
	/// Send a signal to a running or created container
	Kill {
		/// Id of the container
		container_id: String,
		/// Signal to be sent to the init process
		#[arg(default_value = "SIGTERM")]
		signal: String,
		/// Send the signal to all container processes
		#[arg(short = 'a', long, default_value_t)]
		all: bool,
	},
	/// Execute new process inside the container
	Exec {
		/// Id of the container
		container_id: String,
		/// Command, which will be executed in the container
		command: String,
		/// Arguments of the command
		command_options: Vec<String>,
	},
	/// Executes the user defined process in a created container
	Start {
		/// Id of the container
		container_id: String,
	},
	/// Lists containers started by runh with the given root
	List,
	/// Init process running inside a newly created container. Do not use outside of runh!
	Init,
	/// Pull an image or a repository from a registry
	Pull {
		/// Image name
		#[arg(value_name = "NAME[:TAG|@DIGEST]")]
		image: String,
		/// path to the root of the bundle directory
		#[arg(short = 'b', long)]
		bundle: PathBuf,
		/// Username for accessing the registry
		#[arg(short = 'u', long)]
		username: Option<String>,
		/// Password for accessing the registry
		#[arg(short = 'p', long)]
		password: Option<String>,
	},
	/// Checkpoint a running container (not supported)
	Checkpoint,
	/// Restore a container from a previous checkpoint (not supported)
	Restore,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(next_line_help = true)]
#[command(propagate_version = true)]
struct Cli {
	/// root directory for storage of vm state
	#[arg(long, default_value = "/run/user/1000/runh", value_name = "ROOT")]
	root: PathBuf,

	/// The logging level of the application
	#[arg(short = 'l', long, default_value_t, value_enum)]
	log_level: LogLevel,

	/// set the log file path
	#[arg(long, value_name = "LOG_PATH")]
	log: Option<PathBuf>,

	/// set the log format
	#[arg(long, default_value_t, value_enum)]
	log_format: LogFormat,

	/// Path to the hermit-environment. Defaults to <runh-root-dir>/hermit
	#[arg(long, value_name = "HERMIT_ENV_PATH")]
	hermit_env: Option<PathBuf>,

	/// Write out any logs to the runh root directory in addition to the specified log path.
	#[arg(long, default_value_t)]
	debug_log: bool,

	/// Copy the container configuration into the runh root directory for debugging.
	#[arg(long, default_value_t)]
	debug_config: bool,

	/// Currently unimplemented!
	#[arg(long)]
	systemd_cgroup: bool,

	#[command(subcommand)]
	command: Commands,
}

pub fn main() {
	std::panic::set_hook(Box::new(|panic_info| {
		error!("PANIC: {}", panic_info);
	}));

	let cli = Cli::parse();
	parse_matches(&cli);
}
