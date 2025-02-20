use clap::ValueEnum;
use log::{set_boxed_logger, set_max_level, Level, LevelFilter, Metadata, Record};
use serde::Deserialize;
use serde::Serialize;
use serde_json::to_string;
use std::fmt;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::prelude::FromRawFd;
use std::path::PathBuf;
use std::string::String;
use std::sync::Mutex;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogFormat {
	#[default]
	Text,
	Json,
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogLevel {
	#[default]
	Info,
	Warn,
	Debug,
	Trace,
	Error,
	Off,
}

impl LogLevel {
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Info => "info",
			Self::Warn => "warn",
			Self::Debug => "debug",
			Self::Trace => "trace",
			Self::Error => "error",
			Self::Off => "off",
		}
	}
}

impl fmt::Display for LogLevel {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.as_str())
	}
}

impl From<LogLevel> for LevelFilter {
	fn from(value: LogLevel) -> Self {
		match value {
			LogLevel::Info => Self::Info,
			LogLevel::Warn => Self::Warn,
			LogLevel::Debug => Self::Debug,
			LogLevel::Trace => Self::Trace,
			LogLevel::Error => Self::Error,
			LogLevel::Off => Self::Off,
		}
	}
}

#[derive(Serialize, Deserialize)]
pub struct LogEntry {
	pub level: String,
	pub msg: String,
	pub time: String,
}

struct RunhLogger<W: Write + Send + 'static> {
	log_file: Mutex<Option<W>>,
	log_file_internal: Mutex<Option<W>>,
	log_format: LogFormat,
}

impl<W: Write + Send + 'static> log::Log for RunhLogger<W> {
	fn enabled(&self, _metadata: &Metadata) -> bool {
		true
	}

	fn log(&self, record: &Record) {
		let mut file_lock = self.log_file.lock().unwrap();
		if self.enabled(record.metadata()) {
			let message = match self.log_format {
				LogFormat::Text => {
					format!("[{}] {}", record.level(), record.args())
				}
				LogFormat::Json => to_string(&LogEntry {
					level: record.level().as_str().to_ascii_lowercase(),
					msg: format!("{}", record.args()),
					time: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
				})
				.unwrap(),
			};
			if let Some(file) = &mut *file_lock {
				if let Err(err) = writeln!(file, "{message}") {
					println!("Warning in logger: {err} Writing to stdout instead!");
					self.print_level(record.level());
					println!(" {}", record.args());
				}
			} else {
				self.print_level(record.level());
				println!(" {}", record.args());
			}
			let mut file_lock_backup = self.log_file_internal.lock().unwrap();
			if let Some(file_backup) = &mut *file_lock_backup {
				writeln!(file_backup, "{message}").expect("Could not write to backup log file!");
			}
		}
	}

	fn flush(&self) {}
}

impl<W: Write + Send + 'static> RunhLogger<W> {
	/// To improve the readability, every log level
	/// get its own color. This helper function
	/// prints the log level with its associated color.
	fn print_level(&self, level: Level) {
		match level {
			Level::Info => {
				green!("[{}]", level);
			}
			Level::Debug => {
				blue!("[{}]", level);
			}
			Level::Error => {
				red!("[{}]", level);
			}
			Level::Warn => {
				yellow!("[{}]", level);
			}
			_ => {
				black!("{}", level);
			}
		}
	}
}

pub fn init(
	project_dir: PathBuf,
	log_path: Option<PathBuf>,
	log_format: LogFormat,
	log_level: LogLevel,
	internal_log: bool,
) {
	let mut has_log_pipe = false;
	let log_file = log_path
		.map(|path| std::fs::File::create(path).expect("Could not create new log file!"))
		.or_else(|| {
			if let Ok(log_fd) = std::env::var("RUNH_LOG_PIPE") {
				let pipe_fd: i32 = log_fd.parse().expect("RUNH_LOG_PIPE was not an integer!");
				has_log_pipe = true;
				unsafe { Some(File::from_raw_fd(pipe_fd)) }
			} else {
				None
			}
		});

	let logger: RunhLogger<File> = RunhLogger {
		log_file: Mutex::new(log_file),
		log_file_internal: Mutex::new(if has_log_pipe || !internal_log {
			None
		} else {
			Some(
				OpenOptions::new()
					.create(true)
					.truncate(true)
					.write(true)
					.open(project_dir.join(format!(
						"log-{}.json",
						OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
					)))
					.expect("Could not open tmp log file!"),
			)
		}),
		log_format,
	};

	set_boxed_logger(Box::new(logger)).expect("Can't initialize logger");
	set_max_level(log_level.into());

	debug!("Runh logger initialized!");
}
