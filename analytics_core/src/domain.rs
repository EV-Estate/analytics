use ev_lib::architecture::{AggregateRoot, Entity, Id, Specification};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type BuildingId = Id<BuildingTag>;
pub type FileId = Id<FileTag>;

use crate::error::DomainError;

pub struct FileTag;

pub struct BuildingTag;

/// Our acquisition lifecycle for a single lot. `Purchased` carries the UTC instant we
/// bought it — jiff models a UTC instant as `Timestamp` (there is no `DateTime<Utc>`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PropertyState {
	Purchased(Timestamp),
	Interesting,
	Purchasing,
}

impl PropertyState {
	pub fn kind(self) -> PropertyStateKind {
		match self {
			Self::Purchased(_) => PropertyStateKind::Purchased,
			Self::Interesting => PropertyStateKind::Interesting,
			Self::Purchasing => PropertyStateKind::Purchasing,
		}
	}
}

/// The state *category*, stripped of the purchase instant — what filters, badges and
/// map colours switch on. `PropertyState::kind` projects onto it, and it is the value
/// persisted in the `state` text column.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, strum::AsRefStr, strum::Display, strum::EnumString)]
#[strum(serialize_all = "title_case")]
pub enum PropertyStateKind {
	Purchased,
	Interesting,
	Purchasing,
}

/// Build progress, orthogonal to `PropertyState` (which tracks *our* acquisition
/// lifecycle, not the asset's physical state).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, strum::AsRefStr, strum::Display, strum::EnumString)]
#[strum(serialize_all = "title_case")]
pub enum ConstructionStatus {
	UnderConstruction,
	Completed,
}

/// A developer we know. Referenced by `Property::developer` (by `name`); the store
/// enforces that every non-null reference resolves to one of these.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Developer {
	pub name: String,
	//TODO: generalize this `note` into a reusable concept — an arbitrary (table,
	// key) → note side-table surfaced on hover, so any value (not just developers)
	// can carry one, and ideally the lookup tables themselves are defined/managed
	// through it. For now it lives only on Developer.
	pub note: String,
	/// The developer's own homepage. Per-property brochures live in documents, not
	/// here — this is the developer-level link only.
	pub page: Option<String>,
}

/// A Google Place ID — our canonical handle on a property's location. The map
/// resolves it to a pin via Google; name / address / coordinates are derived from
/// it at render time rather than stored by hand.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct GooglePlace(String);

impl GooglePlace {
	pub fn parse(raw: String) -> Result<Self, DomainError> {
		let trimmed = raw.trim();
		if trimmed.is_empty() {
			return Err(DomainError::Validation("google place id must not be empty".into()));
		}
		Ok(Self(trimmed.to_owned()))
	}

	pub fn as_str(&self) -> &str {
		&self.0
	}
}

/// Required link to the research backing a property. A value object so the one
/// real invariant — a non-empty http(s) URL — is enforced at the boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ResearchUrl(String);

impl ResearchUrl {
	pub fn parse(raw: String) -> Result<Self, DomainError> {
		let trimmed = raw.trim();
		if trimmed.is_empty() {
			return Err(DomainError::Validation("research url must not be empty".into()));
		}
		if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
			return Err(DomainError::Validation("research url must be http(s)".into()));
		}
		Ok(Self(trimmed.to_owned()))
	}

	pub fn as_str(&self) -> &str {
		&self.0
	}
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Money(f64);

impl Money {
	pub fn parse(raw: f64) -> Result<Self, DomainError> {
		if raw < 0.0 || !raw.is_finite() {
			return Err(DomainError::Validation("money must be a non-negative finite number".into()));
		}
		Ok(Self(raw))
	}

	pub fn amount(self) -> f64 {
		self.0
	}
}

impl std::fmt::Display for Money {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "${}", v_utils::LargeNumber::new(self.0))
	}
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DealStructure {
	pub equity_pct: f64,
	pub debt_pct: f64,
	pub notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LoanRates {
	pub rate_pct: f64,
	pub term_years: u32,
	pub lender: String,
}

/// A building / development. Per-unit economics live on its `apartments`; the
/// building only carries what is shared across every lot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Building {
	pub id: BuildingId,
	pub name: String,
	pub place: GooglePlace,
	pub construction: ConstructionStatus,
	/// The building's headline target rate (% per year), surfaced as "Target Yield".
	/// `0` means unset — the panel renders it as "-".
	pub target_appreciation: f64,
	/// Developer name; must resolve to a row in the developers table when set.
	pub developer: Option<String>,
	pub research_url: ResearchUrl,
	pub terms: Option<String>,
	pub deal: Option<DealStructure>,
	pub loan: Option<LoanRates>,
	pub additional_reasoning: Option<String>,
	/// The lots inside the building. Synthesised per building until a real per-lot
	/// source exists (`store::mock_apartments`); `number` is 1-based and stable.
	pub apartments: Vec<Apartment>,
	/// Lat/lng for `place`, resolved server-side from a monthly-refreshed on-disk
	/// cache (`<data_dir>/<id>/place.json`). `None` while unresolved — the map
	/// simply draws no pin. Never persisted to the DB.
	#[serde(default)]
	pub coords: Option<Coords>,
}
impl Building {
	/// The distinct portfolio-relationship kinds present across our lots — what map
	/// pins and the state filter switch on. Empty when we own nothing here.
	pub fn state_kinds(&self) -> impl Iterator<Item = PropertyStateKind> {
		let mut kinds: Vec<PropertyStateKind> = Vec::new();
		for a in &self.apartments {
			if let Some(k) = a.status.portfolio_kind()
				&& !kinds.contains(&k)
			{
				kinds.push(k);
			}
		}
		kinds.into_iter()
	}

	/// Mean asking price across lots with a known price; `None` if none are priced.
	pub fn avg_price(&self) -> Option<Money> {
		let priced: Vec<f64> = self.apartments.iter().filter_map(|a| a.price.map(|m| m.amount())).collect();
		if priced.is_empty() {
			return None;
		}
		Money::parse(priced.iter().sum::<f64>() / priced.len() as f64).ok()
	}

	pub fn lots_total(&self) -> usize {
		self.apartments.len()
	}

	/// Off the market: everything not `Available` and not `Interesting` (we treat a
	/// watched lot as still available to the market).
	pub fn lots_sold(&self) -> usize {
		self.lots_total() - self.lots_available()
	}

	pub fn lots_available(&self) -> usize {
		self.apartments
			.iter()
			.filter(|a| matches!(a.status, ApartmentStatus::Available | ApartmentStatus::Interesting))
			.count()
	}

	/// Realized appreciation over the trailing year, in % — the building's mean weekly
	/// value now vs. ~12 months earlier. `None` unless the combined `price_series`
	/// (populated only by `api::get_building`) spans at least a year; the panel then
	/// shows "-".
	pub fn appreciation_yoy(&self) -> Option<f64> {
		use std::collections::BTreeMap;
		const WEEK: i64 = 7 * 24 * 3600;
		const YEAR: i64 = 365 * 24 * 3600;

		let mut weekly: BTreeMap<i64, (f64, u32)> = BTreeMap::new();
		for a in &self.apartments {
			for (t, v) in &a.price_series {
				let e = weekly.entry(t.as_second() / WEEK).or_default();
				e.0 += *v;
				e.1 += 1;
			}
		}
		let series: Vec<(i64, f64)> = weekly.into_iter().map(|(w, (sum, n))| (w * WEEK, sum / n as f64)).collect();
		let first = *series.first()?;
		let last = *series.last()?;
		if last.0 - first.0 < YEAR {
			return None;
		}
		let target = last.0 - YEAR;
		let v_year_ago = series.iter().min_by_key(|(t, _)| (t - target).abs()).map(|(_, v)| *v)?;
		Some((last.1 / v_year_ago - 1.0) * 100.0)
	}

	/// Fraction of all lots that are ours (`Purchasing` or `Purchased`) — the donut's
	/// centred "your share".
	pub fn your_share(&self) -> f64 {
		let total = self.lots_total();
		if total == 0 {
			return 0.0;
		}
		let ours = self
			.apartments
			.iter()
			.filter(|a| matches!(a.status, ApartmentStatus::Purchasing | ApartmentStatus::Purchased(_)))
			.count();
		ours as f64 / total as f64
	}
}

/// One lot inside a building. `status` is its market state fused with our portfolio
/// relationship; `price` / `price_series` are per-unit.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Apartment {
	/// 1-based, stable within a building; the `appt=` URL index.
	pub number: u32,
	pub status: ApartmentStatus,
	/// `None` until we have a real number — rendered as a `?` rather than fabricated.
	pub price: Option<Money>,
	/// Mocked weekly value estimates, filled by `api::get_building`. Never persisted.
	#[serde(default)]
	pub price_series: Vec<(Timestamp, f64)>,
}

/// A lot's baseline market status fused with our relationship to it. `Available` /
/// `Sold` are not ours; the rest project onto a `PropertyStateKind`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum ApartmentStatus {
	Available,
	/// Sold to someone else.
	Sold,
	/// Ours, acquisition in progress.
	Purchasing,
	/// Ours, closed — carries the purchase instant.
	Purchased(Timestamp),
	/// Ours, watching an otherwise-available lot.
	Interesting,
}

impl ApartmentStatus {
	/// Our portfolio relationship to this lot, or `None` when it is not ours.
	pub fn portfolio_kind(self) -> Option<PropertyStateKind> {
		match self {
			Self::Purchased(_) => Some(PropertyStateKind::Purchased),
			Self::Purchasing => Some(PropertyStateKind::Purchasing),
			Self::Interesting => Some(PropertyStateKind::Interesting),
			Self::Available | Self::Sold => None,
		}
	}
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Coords {
	pub lat: f64,
	pub lng: f64,
}

impl Entity for Building {
	type Id = BuildingId;

	fn id(&self) -> BuildingId {
		self.id
	}
}

impl AggregateRoot for Building {
	const NAME: &'static str = "property";
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, strum::AsRefStr, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum FileKind {
	Pic,
	PitchDeck,
	Document,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PropertyFile {
	pub id: FileId,
	pub building_id: BuildingId,
	/// The owning lot, or `None` for a building-level file.
	pub apt: Option<u32>,
	pub kind: FileKind,
	pub filename: String,
	pub content_type: String,
}

/// Map / portfolio filter over a building's portfolio relationship. A building holds
/// for `InState(k)` when any of its lots is ours in that kind.
pub struct InState(pub PropertyStateKind);

impl Specification<Building> for InState {
	fn holds(&self, candidate: &Building) -> bool {
		candidate.state_kinds().any(|k| k == self.0)
	}
}

// Free functions, not inherent impls: `Id` is a foreign type, so coherence
// forbids `impl BuildingId { .. }` here.
pub fn parse_building_id(raw: &str) -> Result<BuildingId, DomainError> {
	Uuid::parse_str(raw)
		.map(BuildingId::from_raw)
		.map_err(|e| DomainError::Validation(format!("invalid building id: {e}")))
}

pub fn parse_file_id(raw: &str) -> Result<FileId, DomainError> {
	Uuid::parse_str(raw).map(FileId::from_raw).map_err(|e| DomainError::Validation(format!("invalid file id: {e}")))
}
