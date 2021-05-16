use std::fs::File;
use std::io::prelude::*;

use crate::cri::runtime;

pub fn create_spec(path: std::path::PathBuf) -> std::io::Result<()> {
	let spec: runtime::Spec = Default::default();

	let mut file = File::create(path)?;
	write!(file, "{:?}", spec)?;

	Ok(())
}
