//! Per-record translations for the seeded property dataset.
//!
//! The dataset in [`store`](crate::store) is authored per building, not chrome:
//! project names, developers, terms, reasoning. Names and developers are proper
//! nouns and stay as written. The two prose fields — `terms` and
//! `additional_reasoning` — are what a reader actually reads, and they live
//! here, in a sidecar keyed by the building's (stable) UUID.
//!
//! ## Why a sidecar and not the catalogue
//!
//! `messages/*/common.json` is this surface's *chrome*. A property's terms are
//! content: they are edited per building, they arrive from the seed rather than
//! from a designer, and there are as many of them as there are buildings. Mixing
//! the two would mean a catalogue that grows with the portfolio. This is the
//! same shape the conductor uses for publication cards.
//!
//! ## Provenance, per field
//!
//! Each entry stores the English it was translated from. A field whose English
//! has since moved is refused and the current English renders instead — a
//! translation of a sentence nobody writes any more is worse than the sentence.
//! Per *field*, because `terms` and `reasoning` are edited independently.

use std::{collections::HashMap, sync::LazyLock};

use analytics_core::domain::Building;
use ev_lib::i18n::Locale;
use serde::Deserialize;

/// building id → locale → field → entry.
type Sidecar = HashMap<String, HashMap<String, HashMap<String, Entry>>>;
/// Handover / legal terms, in this locale where one exists.
pub fn terms(building: &Building, locale: Locale) -> Option<&str> {
	building.terms.as_deref().map(|current| field(building, locale, "terms", current))
}
/// The investment rationale, in this locale where one exists.
pub fn reasoning(building: &Building, locale: Locale) -> Option<&str> {
	building.additional_reasoning.as_deref().map(|current| field(building, locale, "reasoning", current))
}
/// `{ en, t }` — the same provenance shape the message catalogues use.
#[derive(Deserialize)]
struct Entry {
	en: String,
	t: String,
}

static SIDECAR: LazyLock<Sidecar> = LazyLock::new(|| serde_json::from_str(include_str!("translations.json")).expect("translations.json is well-formed"));

/// The field as this locale should see it.
///
/// Never blanks and never panics: an unknown building, a missing locale, a
/// missing field and a stale entry all resolve to the English already on the
/// record. `En` short-circuits — English is the source, not a translation of
/// itself.
fn field<'a>(building: &'a Building, locale: Locale, name: &str, current: &'a str) -> &'a str {
	if matches!(locale, Locale::En) {
		return current;
	}
	SIDECAR
		.get(&building.id.to_string())
		.and_then(|locales| locales.get(locale.code()))
		.and_then(|fields| fields.get(name))
		// Refuse a translation whose English source has moved on.
		.filter(|entry| entry.en == current)
		.map_or(current, |entry| entry.t.as_str())
}

#[cfg(test)]
mod tests {
	use analytics_core::domain::{Building, BuildingId, ConstructionStatus, GooglePlace, ResearchUrl};
	use ev_lib::i18n::Locale;
	use uuid::Uuid;

	use super::*;

	/// A real seeded id, so the sidecar actually has an entry for it.
	const MELODY: &str = "9a5a7d3b-cd42-4d65-a426-5b705a3d0cc9";

	fn building(id: &str, terms: Option<&str>) -> Building {
		Building {
			id: BuildingId::from_raw(Uuid::parse_str(id).unwrap()),
			name: "x".into(),
			place: GooglePlace::parse("place".into()).unwrap(),
			construction: ConstructionStatus::Completed,
			target_appreciation: 0.0,
			developer: None,
			research_url: ResearchUrl::parse("https://x.test".into()).unwrap(),
			terms: terms.map(str::to_owned),
			deal: None,
			loan: None,
			additional_reasoning: None,
			apartments: Vec::new(),
			coords: None,
		}
	}

	/// The English the sidecar was written against, read back out of it.
	fn seeded_en(field: &str) -> String {
		SIDECAR[MELODY]["ru"][field].en.clone()
	}

	#[test]
	fn every_seeded_building_has_all_four_locales() {
		for (id, locales) in SIDECAR.iter() {
			for locale in ["ru", "vi", "fr", "de"] {
				let fields = locales.get(locale).unwrap_or_else(|| panic!("{id} missing {locale}"));
				for field in ["terms", "reasoning"] {
					let entry = fields.get(field).unwrap_or_else(|| panic!("{id}/{locale} missing {field}"));
					assert!(!entry.t.trim().is_empty(), "{id}/{locale}/{field} is blank");
					assert_ne!(entry.t, entry.en, "{id}/{locale}/{field} was never translated");
				}
			}
		}
	}

	#[test]
	fn english_is_the_source_not_a_translation_of_itself() {
		let en = seeded_en("terms");
		let b = building(MELODY, Some(&en));
		assert_eq!(terms(&b, Locale::En), Some(en.as_str()));
	}

	#[test]
	fn a_matching_record_is_translated() {
		let en = seeded_en("terms");
		let b = building(MELODY, Some(&en));
		let got = terms(&b, Locale::Ru).unwrap();
		assert_ne!(got, en, "the Russian terms should not be the English ones");
		assert_eq!(got, SIDECAR[MELODY]["ru"]["terms"].t);
	}

	/// Rule 1.2: the entry records the English it was translated from, so an
	/// edit to the seed withdraws its own stale translations.
	#[test]
	fn a_moved_english_source_refuses_its_translation() {
		let b = building(MELODY, Some("Handed over early 2024, but this sentence was since rewritten."));
		assert_eq!(terms(&b, Locale::Ru), b.terms.as_deref());
	}

	#[test]
	fn an_unknown_building_falls_back_to_english() {
		let b = building("00000000-0000-4000-8000-000000000000", Some("Something in English."));
		assert_eq!(terms(&b, Locale::Ru), Some("Something in English."));
	}

	#[test]
	fn an_absent_field_stays_absent() {
		let b = building(MELODY, None);
		assert_eq!(terms(&b, Locale::Ru), None);
	}
}
