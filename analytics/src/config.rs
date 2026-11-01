use std::net::SocketAddr;

use secrecy::SecretString;
use smart_default::SmartDefault;
use v_utils::{io::ExpandedPath, macros as v_macros};

#[derive(Clone, Debug, SmartDefault, v_macros::LiveSettings, v_macros::MyConfigPrimitives, v_macros::Settings)]
pub struct AppConfig {
	pub maps_api_key: SecretString,
	#[default(app_data("app.db"))]
	pub db_path: ExpandedPath,
	pub admin_token: SecretString,
	#[serde(default)]
	pub admins: Vec<String>,
	/// Address the server binds to.
	#[default(SocketAddr::from(([127, 0, 0, 1], 59079)))]
	pub socket_addr: SocketAddr,
}

/// The DB (which is 100% of state) lives outside the repo, under
/// `$XDG_DATA_HOME/analytics/` so a checkout never carries it. Prod
/// overrides this to the `/data` PVC via `deploy/config.nix`; real content
/// arrives through litestream, not the repo.
fn app_data(file: &str) -> ExpandedPath {
	ExpandedPath::from(format!("{}/{}/{file}", v_utils::io::xdg::xdg_data_fallback(), env!("CARGO_PKG_NAME")))
}
