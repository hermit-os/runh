use dkregistry::reference::Reference;
use dkregistry::render;
use dkregistry::v2::Client;
use futures::future::try_join_all;
use std::str::FromStr;
use std::string::String;
use tokio::runtime::Runtime;

async fn async_pull(
	registry: &str,
	username: Option<String>,
	password: Option<String>,
	bundle: Option<String>,
) {
	debug!(
		"Try to pull image with username: {:?} and password: {:?}",
		username, password
	);
	let dkref = Reference::from_str(registry).expect("Invalid registry string");
	debug!("Select {}", dkref);

	let image = &dkref.repository().clone();
	let login_scope = format!("repository:{}:pull", image);
	let version = &dkref.version().clone();
	let dclient = Client::configure()
		.registry(&dkref.registry().clone())
		.insecure_registry(false)
		.username(username)
		.password(password)
		.build()
		.expect("Unable to create registry client")
		.authenticate(&[&login_scope])
		.await
		.expect("Athentication failed!");

	let manifest = dclient
		.get_manifest(image, version)
		.await
		.expect("Unable to get manifest");
	let layers_digests = manifest
		.layers_digests(None)
		.expect("Unable to determin the number of layers");
	debug!(
		"{} -> got {} layer(s)",
		&dkref.repository().clone(),
		layers_digests.len()
	);

	let blob_futures = layers_digests
		.iter()
		.map(|layer_digest| dclient.get_blob(&image, &layer_digest))
		.collect::<Vec<_>>();

	let blobs = try_join_all(blob_futures)
		.await
		.expect("Unable to create blobs");

	debug!("Downloaded {} layers", blobs.len());

	let path = std::path::PathBuf::from(bundle.unwrap());
	let can_path = path.canonicalize().unwrap();

	debug!("Unpacking layers to {:?}", &can_path);
	render::unpack(&blobs, &can_path).expect("Unable to unpack blobs");

	println!("Store image in {}", can_path.to_str().unwrap());
}

pub fn pull_registry(
	registry: &str,
	username: Option<&str>,
	password: Option<&str>,
	bundle: Option<&str>,
) {
	Runtime::new()
		.expect("Unable to create Tokio runtime")
		.block_on(async_pull(
			registry,
			username.map(|s| s.to_string()),
			password.map(|s| s.to_string()),
			bundle.map(|s| s.to_string()),
		));
}
