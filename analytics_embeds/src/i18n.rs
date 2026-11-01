//! Locale wiring for the embed bundle.
//!
//! Loading and policy come from [`analytics_core::i18n`], shared
//! with the dashboard. The catalogue does not: this surface's copy is marketing
//! and the dashboard's is operational, and one vocabulary cannot serve both
//! audiences.
//!
//! **Only the labels are translated.** The section intro, the two property names
//! and the TMS street address in [`view`](crate::view) stay as written — they are
//! authored copy and proper nouns, the same class as the dataset in the
//! dashboard's `store`. A translated frame around English prose is the worse of
//! the two failures.
//!
//! `messages/en/common.json` is **generated** — `cargo run --example
//! dump_messages` — because English is authored at the `t!` sites in `view.rs`.
//! The other four are hand-authored in the `{en, t}` envelope.

use analytics_core::i18n::Catalogues;
use dioxus::prelude::*;
use ev_lib::i18n::{Locale, Messages};

pub const CATALOGUES: Catalogues = Catalogues {
	en: include_str!("../messages/en/common.json"),
	ru: include_str!("../messages/ru/common.json"),
	vi: include_str!("../messages/vi/common.json"),
	fr: include_str!("../messages/fr/common.json"),
	de: include_str!("../messages/de/common.json"),
};

/// The `fn(Locale) -> Messages` the `mfe!` macro takes.
pub fn catalogue(locale: Locale) -> Messages {
	CATALOGUES.resolve(locale).messages
}

/// Renders a label whose accent word is marked `*like this*`.
///
/// The heading sets one word in italic serif — "Premium Asset *Portfolio*".
/// Splitting that into two keys would hard-code English word order: Russian and
/// German both put the adjective elsewhere in the phrase, and a translator
/// handed `{lead} <em>{accent}</em>` cannot move it. One key with an inline
/// marker lets the sentence be rearranged and keeps the typography.
#[component]
pub fn Accented(text: String, class: String) -> Element {
	rsx! {
		// Odd segments are the marked ones: "a *b* c" -> ["a ", "b", " c"].
		for (i , part) in text.split('*').enumerate() {
			if i % 2 == 1 {
				span { class: "{class}", "{part}" }
			} else {
				"{part}"
			}
		}
	}
}
