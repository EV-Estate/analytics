#![feature(default_field_values)]
//! The Rust half of `evinvest-i18n-check`: the committed English catalogue must
//! still match the code, and no translation may have drifted from its source.
//!
//! `view` is pulled in by `#[path]` rather than through the crate because
//! `src/lib.rs` is `#![cfg(target_arch = "wasm32")]` — and because `t!`
//! registration is a linker fact, so the sites have to be linked into *this*
//! binary to be seen. Nothing is rendered here; that is the point.

#[path = "../src/messages.rs"]
mod messages;
#[path = "../src/view.rs"]
mod view;

use ev_lib::i18n::{LOCALES, Locale, policy};

#[test]
fn the_committed_english_catalogue_matches_the_code() {
	let linked = ev_lib::i18n::catalogue().unwrap_or_else(|conflicts| panic!("{}", conflicts.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")));
	let committed = include_str!("../messages/en/common.json");

	// Compared as bytes, not as maps: the committed file is what
	// `resolve_catalogue` compares every translation against, and what the
	// conductor's `i18n:check` would read if these catalogues were ever shared.
	assert_eq!(
		messages::serialise(&linked),
		committed,
		"messages/en/common.json is out of date with the t! sites in view.rs — run \
		 `cargo run -p real_estate_allocation_embeds --example dump_messages`"
	);
}

#[test]
fn no_translation_has_drifted_from_its_english_source() {
	let en = messages::source();
	let resolved: Vec<_> = LOCALES
		.into_iter()
		.filter(|&l| l != Locale::En)
		.map(|locale| policy::resolve_catalogue(locale, &en, &messages::authored(locale)))
		.collect();

	// Floor 0: untranslated keys are reported, not failed. A locale is filled in
	// over time, and blocking CI on an unfinished translation would just get the
	// check disabled. Drift is the fatal half — English served under a locale's
	// chrome is indistinguishable from a surface nobody ever translated.
	let (_, report) = policy::audit(&resolved, 0.0);
	println!("{report}");

	let drifted: Vec<String> = resolved
		.iter()
		.flat_map(|c| c.rejected.iter().map(|r| format!("{}/{}: {:?} — {}", c.locale, r.key, r.reason, r.detail)))
		.collect();
	assert!(
		drifted.is_empty(),
		"English is being served for {} entr{}. Retranslate and update the `en` field, \
		 or revert the English change:\n  {}",
		drifted.len(),
		if drifted.len() == 1 { "y" } else { "ies" },
		drifted.join("\n  ")
	);
}
