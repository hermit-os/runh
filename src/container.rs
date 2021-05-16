use crate::spec::runtime::Spec;
use derive_builder::Builder;
use getset::Getters;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Builder, Getters, Serialize, Deserialize)]
#[builder(default, pattern = "owned", setter(into, strip_option))]
/// A general OCI container implementation.
pub struct OCIContainer {
	#[get = "pub"]
	/// Unique identifier of the container.
	id: String,

	#[get = "pub"]
	/// OCI Runtime Specification of the container.
	spec: Spec,
}
