#![feature(default_field_values)]
#![cfg(target_arch = "wasm32")] // web-sys/gloo-net + `ev_lib::mfe` — wasm-only bundle
//! The overview tile: hosts load this bundle cross-origin from the analytics server's
//! `/mfe/analytics/` and compose `<tag>` directly into their page. All presentation lives
//! in `view` (native-compilable for the i18n catalogue checks); this file is only the
//! wasm-only shell: the custom-element registration (`ev_lib::mfe!`) and the live fetch.
//!
//! Nothing is passed to the element: the locale comes off the host page's `<html lang>`
//! (`ev_lib::mfe::host_locale`), every URL off the bundle's own origin.

mod i18n;
mod view;

use analytics_core::domain::Building;
use dioxus::prelude::*;
use ev_lib::mfe::bundle_origin;
use view::{Featured, Origin, Overview, Q1_PROPERTY};

ev_lib::mfe! {
	service: "analytics", name: "overview", kind: component,
	root: crate::OverviewContainer, stylesheet: "analytics/overview.css",
	messages: crate::i18n::catalogue
}

/// wasm shell around `view::Overview`: fetches Q1's live figures (the server enriches the
/// building with `price_series`, the basis for `appreciation_yoy`) and provides the origin.
#[component]
fn OverviewContainer() -> Element {
	let building = use_resource(|| async move {
		let url = format!("{}/mfe/analytics/api/embed/building/{}", bundle_origin(), Q1_PROPERTY);
		let b: Option<Building> = gloo_net::http::Request::get(&url).send().await.ok()?.json().await.ok()?;
		b
	});
	use_context_provider(|| Origin(bundle_origin()));
	let building = Featured(building.read().clone());
	rsx! { Overview { building } }
}
