//! The Rust half of `evinvest-i18n-check`: the committed English catalogue must
//! still match the code, and no translation may have drifted from its source.
//!
//! Nothing is rendered here, and that is the point — `t!` registration is a
//! linker fact, so a site that compiles into this binary is checked whether or
//! not any test ever draws it.

use analytics::i18n::CATALOGUES;
use ev_lib::i18n::policy;

#[test]
fn the_committed_english_catalogue_matches_the_code() {
	let linked = ev_lib::i18n::catalogue().unwrap_or_else(|conflicts| panic!("{}", conflicts.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")));
	let committed = include_str!("../messages/en/common.json");

	// Compared as bytes, not as maps: the committed file is what
	// `resolve_catalogue` compares every translation against, and what the
	// conductor's `i18n:check` would read if these catalogues were ever shared.
	assert_eq!(
		analytics_core::i18n::serialise(&linked),
		committed,
		"messages/en/common.json is out of date with the t! sites — run \
		 `cargo run -p analytics --example dump_messages`"
	);
}

#[test]
fn no_translation_has_drifted_from_its_english_source() {
	let resolved = CATALOGUES.resolve_all();

	// Floor 0: untranslated keys are reported, not failed. A locale is filled in
	// over time, and blocking CI on an unfinished translation would just get the
	// check disabled. Drift is the fatal half — English served under a locale's
	// chrome is indistinguishable from a surface nobody ever translated.
	let (_, report) = policy::audit(&resolved, 0.0);
	println!("{report}");

	let drifted: Vec<String> = resolved
		.iter()
		.flat_map(|c| c.rejected.iter().map(|r| format!("{}/{}: {} — {}", c.locale, r.key, r.reason, r.detail)))
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
