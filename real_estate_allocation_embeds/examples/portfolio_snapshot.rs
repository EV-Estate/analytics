#![feature(default_field_values)]
//! Native SSR of the Portfolio embed → a self-contained static HTML snapshot, baked by
//! the flake as `portfolio.<locale>.html` and served by the conductor as the
//! `RemoteElement` fallback (shown until/unless the live wasm bundle upgrades). Rendered
//! from the SAME `view` components the live bundle mounts, so it can't drift.
//!
//! **One snapshot per locale, not one snapshot.** Per the component-MFE snapshot
//! contract the fallback shows *permanently* if the bundle never upgrades — so a single
//! `<html lang="en">` file served on `/ru` is not a flash of English, it is English
//! forever. The locale is the first argument; the conductor picks the file by page
//! locale, and `loadDocHtml` asserts it exists at build.
//!
//! `src/lib.rs` is `#![cfg(target_arch = "wasm32")]` (the cdylib is wasm-only), so `view`
//! and `messages` are pulled in directly rather than through the crate — they compile
//! natively here.
//!
//! Build-time inputs stand in for the wasm-only ones: `AssetOrigin("")` (root-relative,
//! the conductor serves `/mfe/seed/...`) and `building = None` (no live fetch → the three
//! featured stats render the standard `MISSING` placeholder). Dark-only; the conductor is
//! dark-only, so there is no light variant.

#[path = "../src/messages.rs"]
mod messages;
#[path = "../src/view.rs"]
mod view;

use dioxus::prelude::*;
use ev_lib::i18n::{Locale, Translator};
use view::{AssetOrigin, Featured, Overview};

fn main() {
	let arg = std::env::args().nth(1).expect("usage: portfolio_snapshot <locale>");
	let locale: Locale = arg.parse().unwrap_or_else(|e| panic!("{e}"));

	let css = std::env::var("SNAPSHOT_CSS")
		.map(|p| std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("SNAPSHOT_CSS={p}: {e}")))
		.expect("SNAPSHOT_CSS must point at the compiled mfe.css (set by the flake installPhase)");

	let mut dom = VirtualDom::new_with_props(Snapshot, SnapshotProps { locale });
	dom.rebuild_in_place();
	let body = dioxus_ssr::render(&dom);

	// Self-contained: a minimal reset + the inlined mfe.css + dark `color-scheme`. No host
	// fonts (mfe.css ships none; system-font fallback is acceptable for a fallback tile).
	print!(
		"<!doctype html>\n<html lang=\"{locale}\"><head>\
<meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<style>:root{{color-scheme:dark}}*{{box-sizing:border-box}}html,body{{margin:0}}</style>\
<style>{css}</style>\
</head><body style=\"background:#070d18\">{body}</body></html>\n"
	);
}
#[component]
fn Snapshot(locale: Locale) -> Element {
	use_context_provider(|| AssetOrigin(String::new()));
	// The snapshot's stand-in for what `mfe!` does from the host page's `lang`.
	use_context_provider(|| Translator::new(messages::catalogue(locale), locale));
	rsx! { Overview { building: Featured(None) } }
}
