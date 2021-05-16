use clap::{crate_authors, crate_description, crate_version, AppSettings, Clap};
use derive_builder::Builder;
use getset::{CopyGetters, Getters};
use serde::{Deserialize, Serialize};
use log::LevelFilter;
use crate::spec::runtime;

#[derive(Builder, Clap, CopyGetters, Getters)]
#[builder(default, pattern = "owned", setter(into, strip_option))]
#[clap(
    about(crate_description!()),
    author(crate_authors!("\n")),
    after_help("More info at: https://github.com/hermitcore/runh"),
    global_setting(AppSettings::ColoredHelp),
    version(crate_version!()),
)]
/// Config is the main configuration structure for the server.
pub struct Config {
    #[get_copy = "pub"]
    #[clap(
        default_value("info"),
        env("RUNH_LOG_LEVEL"),
        long("log-level"),
        possible_values(&["trace", "debug", "info", "warn", "error", "off"]),
        short('l'),
        value_name("LEVEL")
    )]
    /// The logging level of the application.
    log_level: LevelFilter,

    #[clap(subcommand)]
    subcmd: SubCommand,
}

impl Default for Config {
    fn default() -> Self {
        Self::parse()
    }
}

#[derive(Clap)]
pub enum SubCommand {
    #[clap(
        version = "1.3",
        help = "",
    )]
    Spec(OciSpec),
}

/// Create a new specification file
#[derive(Clap)]
pub struct OciSpec;