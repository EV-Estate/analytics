//! Locale wiring for the dashboard.
//!
//! The catalogue loading and the policy live in
//! [`analytics_core::i18n`], shared with the embed bundle. What is
//! here is only what differs per surface: this crate's own `messages/`, which the
//! page bundle's `mfe!` resolves against the host's `<html lang>`.
//!
//! The property dataset in [`store`](crate::store) is content authored per
//! building, not chrome, so it does not live here. Its two prose fields — terms
//! and reasoning — are translated per record in [`translations`](crate::translations),
//! the same route publication cards took. Project names and developers are
//! proper nouns and stay as written.

use analytics_core::{domain::ApartmentStatus, i18n::Catalogues};
use dioxus::prelude::*;
use ev_lib::{
	i18n::{DEFAULT_LOCALE, Locale, Messages, Translator},
	t,
};

/// This surface's copy: operational, not marketing. The embed bundle keeps its
/// own for the opposite reason.
pub const CATALOGUES: Catalogues = Catalogues {
	en: include_str!("../messages/en/common.json"),
	ru: include_str!("../messages/ru/common.json"),
	vi: include_str!("../messages/vi/common.json"),
	fr: include_str!("../messages/fr/common.json"),
	de: include_str!("../messages/de/common.json"),
};
/// The `fn(Locale) -> Messages` the `mfe!` macro takes: it resolves the host page's
/// `<html lang>` and provides the one `Translator` above the dashboard.
pub fn catalogue(locale: Locale) -> Messages {
	CATALOGUES.resolve(locale).messages
}
/// The translator for the current locale.
///
/// ```ignore
/// let tr = use_t();
/// rsx! { span { {t!(tr, "panel.map", "Map")} } }
/// ```
pub fn use_t() -> Translator {
	// `use_context` panics when the provider is absent, which would turn a panel
	// rendered outside the bundle root — a prerender of one component, a story —
	// into a crash over a *label*. Degrade to the canonical locale instead.
	match try_consume_context::<Translator>() {
		Some(tr) => tr,
		None => Translator::new(catalogue(DEFAULT_LOCALE), DEFAULT_LOCALE),
	}
}
/// An apartment's portfolio state, rendered. One mapping for the whole
/// dashboard: the header and the details panel each holding their own is how a
/// building ends up "Purchased" in one place and something else in another.
///
/// Returns the label rather than a key because `t!` takes literals — the key and
/// its English are one unit, and a `&'static str` key handed around would leave
/// the English stranded in a catalogue the compiler cannot see.
pub fn status_label(tr: &Translator, status: ApartmentStatus) -> String {
	match status {
		ApartmentStatus::Available => t!(tr, "status.available", "Available"),
		ApartmentStatus::Sold => t!(tr, "status.sold", "Sold"),
		ApartmentStatus::Purchasing => t!(tr, "status.purchasing", "Purchasing"),
		ApartmentStatus::Purchased(_) => t!(tr, "status.purchased", "Purchased"),
		ApartmentStatus::Interesting => t!(tr, "status.interesting", "Interesting"),
	}
}
