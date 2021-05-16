use log::{set_logger, set_max_level, Level, LevelFilter, Metadata, Record};
struct RunhLogger;

impl log::Log for RunhLogger {
	fn enabled(&self, _metadata: &Metadata) -> bool {
		true
	}

	fn log(&self, record: &Record) {
		if self.enabled(record.metadata()) {
			self.print_level(record.level());
			println!(" {}", record.args());
		}
	}

	fn flush(&self) {}
}

impl RunhLogger {
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

pub fn init(log_level: Option<&str>) {
	set_logger(&RunhLogger).expect("Can't initialize logger");
	let max_level: LevelFilter = match log_level {
		Some("error") => LevelFilter::Error,
		Some("debug") => LevelFilter::Debug,
		Some("off") => LevelFilter::Off,
		Some("trace") => LevelFilter::Trace,
		Some("warn") => LevelFilter::Warn,
		Some("info") => LevelFilter::Info,
		_ => LevelFilter::Info,
	};
	set_max_level(max_level);
}
