#![feature(default_field_values)]
#![cfg(target_arch = "wasm32")] // web-sys/gloo-net + `ev_lib::mfe` — wasm-only bundle
//! Marketing surface and cross-origin microfrontend bundle. The wasm container for the
//! landing "Premium Asset Portfolio" bento section — no app shell, so the landing host
//! composes `<tag>` directly into its page. All presentation lives in `view` (native-
//! compilable, so the same components render the static `portfolio.<locale>.html` snapshots); this
//! file is only the wasm-only shell: the custom-element registration (`ev_lib::mfe!`),
//! the live data fetch, and the asset-origin derivation.
//!
//! The locale is not among the wasm-only inputs, and deliberately so: `mfe!` reads
//! it off the host page's `lang` (`ev_lib::mfe::host_locale`) and provides the
//! `Translator` above `view`. An attribute could not carry it — the conductor's
//! `RemoteElement` appends the element in one effect and sets its attributes in a
//! later one, so anything pushed in is absent at `connectedCallback` time.

mod messages;
mod view;

use dioxus::prelude::*;
use ev_lib::mfe::bundle_origin;
use real_estate_allocation_core::domain::Building;
use view::{AssetOrigin, Featured, Overview, Q1_PROPERTY};

// The producer entrypoint: generates the custom-element registration, the
// `wasm-bindgen(start)` entrypoint, the origin self-derivation, and `MFE_MANIFEST`
// (emitted by the build as `mfe.json`).
ev_lib::mfe! {
	service: "real-estate", name: "overview", kind: component,
	root: crate::OverviewContainer, stylesheet: "mfe.css",
	messages: crate::messages::catalogue
}

/// wasm shell around `view::Overview`: fetches Q1's live figures, provides the asset
/// origin (banner URLs resolve against wherever the `.js` loaded from), then renders the
/// pure presentation. The SSR snapshot takes the same `Overview` with `building=None` and
/// `AssetOrigin("")` — hence the split.
#[component]
fn OverviewContainer() -> Element {
	// Pull Q1's live figures so the headline stats track the DB rather than hard-coded
	// copy. The server enriches the building with `price_series` (the basis for
	// `appreciation_yoy`) before serializing — wire-compatible with `core::domain::Building`.
	let building = use_resource(|| async move {
		let url = format!("{}/api/embed/building/{}", rea_origin(), Q1_PROPERTY);
		let b: Option<Building> = gloo_net::http::Request::get(&url).send().await.ok()?.json().await.ok()?;
		b
	});
	use_context_provider(|| AssetOrigin(bundle_origin()));
	let building = Featured(building.read().clone());
	rsx! { Overview { building } }
}

/// REA backend origin for the data fetch. The host page advertises it via
/// `<meta name="rea-url">` (its `NEXT_PUBLIC_REA_URL`); absent it, fall back to the
/// bundle's own origin (REA serving the bundle itself). A plain DOM read —
/// `web-sys` is already linked by dioxus-web, so this costs the bundle nothing.
fn rea_origin() -> String {
	web_sys::window()
		.and_then(|w| w.document())
		.and_then(|d| d.query_selector("meta[name=\"rea-url\"]").ok().flatten())
		.and_then(|m| m.get_attribute("content"))
		.filter(|s| !s.is_empty())
		.unwrap_or_else(bundle_origin)
}
