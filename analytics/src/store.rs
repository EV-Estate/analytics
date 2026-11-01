use std::{
	path::{Path, PathBuf},
	str::FromStr as _,
};

use async_trait::async_trait;
use ev_lib::architecture::{Reader, Repository, Specification};
use sqlx::{
	FromRow, Row as _, SqlitePool,
	sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

use crate::{
	domain::{Apartment, ApartmentStatus, Building, BuildingId, ConstructionStatus, Developer, FileId, FileKind, GooglePlace, Money, PropertyFile, PropertyState, ResearchUrl},
	error::DomainError,
};

/// Leaf port over the `ev_lib` repository markers. No `UnitOfWork`: every write here
/// is a single row, so a transaction boundary would buy nothing.
#[async_trait]
pub trait BuildingRepository: Repository<Aggregate = Building> + Reader<Aggregate = Building> {
	async fn list(&self, spec: Option<&(dyn Specification<Building> + Sync)>) -> Result<Vec<Building>, DomainError>;
	async fn get(&self, id: BuildingId) -> Result<Option<Building>, DomainError>;
	/// Persist a building aggregate whole. The `developer` reference (if any) is
	/// validated against the developers table here — a dangling one is a `Validation`
	/// error, never silently written.
	async fn put(&self, b: &Building) -> Result<(), DomainError>;
	async fn add_file(&self, f: PropertyFile, content: &[u8]) -> Result<(), DomainError>;
	async fn file_content(&self, id: FileId) -> Result<Vec<u8>, DomainError>;
	async fn list_files(&self, id: BuildingId) -> Result<Vec<PropertyFile>, DomainError>;
	async fn get_developer(&self, name: &str) -> Result<Option<Developer>, DomainError>;
}

#[derive(Clone)]
pub struct SqliteStore {
	pool: SqlitePool,
}

impl SqliteStore {
	pub async fn open(db_path: &Path) -> Result<Self, DomainError> {
		if let Some(parent) = db_path.parent() {
			std::fs::create_dir_all(parent).map_err(|e| DomainError::Repository(format!("create db dir: {e}")))?;
		}

		// `foreign_keys(true)` per connection so `property_files.property_id` →
		// properties(id) is enforced by the DB, not just by discipline.
		// WAL is required by litestream (the sidecar that replicates `/data/app.db` to R2);
		// sqlx otherwise leaves the journal mode unset, which defaults to SQLite's DELETE mode.
		let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
			.map_err(map_sqlx_error)?
			.create_if_missing(true)
			.foreign_keys(true)
			.journal_mode(SqliteJournalMode::Wal);
		let pool = SqlitePoolOptions::new().connect_with(opts).await.map_err(map_sqlx_error)?;
		// Schema is versioned in `migrations/`; applied here so a bare server boot keeps
		// the DB current, and via `db migrate` for explicit/CI use. The `Building` doc
		// shape stays owned by the Rust struct — migrations only touch table structure.
		sqlx::migrate!().run(&pool).await.map_err(|e| DomainError::Repository(format!("migrate: {e}")))?;
		Ok(Self { pool })
	}

	/// The saved dock arrangement for (user, band group); `""` user = the global
	/// layout until per-user arrives.
	pub async fn load_layout(&self, user: &str, breakpoint: &str) -> Result<Option<String>, DomainError> {
		let row = sqlx::query("SELECT doc FROM layouts WHERE user = ? AND breakpoint = ?")
			.bind(user)
			.bind(breakpoint)
			.fetch_optional(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		Ok(row.map(|r| r.get("doc")))
	}

	pub async fn save_layout(&self, user: &str, breakpoint: &str, doc: &str) -> Result<(), DomainError> {
		sqlx::query("INSERT INTO layouts (user, breakpoint, doc) VALUES (?, ?, ?) ON CONFLICT (user, breakpoint) DO UPDATE SET doc = excluded.doc")
			.bind(user)
			.bind(breakpoint)
			.bind(doc)
			.execute(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		Ok(())
	}

	pub async fn place_cache_get(&self, building: BuildingId) -> Result<Option<(String, jiff::Timestamp)>, DomainError> {
		let row = sqlx::query("SELECT doc, fetched_at FROM place_cache WHERE building = ?")
			.bind(building.raw().to_string())
			.fetch_optional(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		row.map(|r| {
			let fetched_at: String = r.get("fetched_at");
			let fetched_at = fetched_at.parse().map_err(|e| corrupt_row(DomainError::Repository(format!("place_cache fetched_at: {e}"))))?;
			Ok((r.get("doc"), fetched_at))
		})
		.transpose()
	}

	pub async fn place_cache_put(&self, building: BuildingId, doc: &str, fetched_at: jiff::Timestamp) -> Result<(), DomainError> {
		sqlx::query("INSERT INTO place_cache (building, doc, fetched_at) VALUES (?, ?, ?) ON CONFLICT (building) DO UPDATE SET doc = excluded.doc, fetched_at = excluded.fetched_at")
			.bind(building.raw().to_string())
			.bind(doc)
			.bind(fetched_at.to_string())
			.execute(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		Ok(())
	}
}

/// Our Quy Nhơn portfolio. The four already-built projects (one per developer
/// track from issue #3, plus the separately-selected Calla) are `Purchased` — the
/// holdings the portfolio view renders. Two under-construction towers (Q1 / Triton)
/// are `Purchasing` prospects. Marketing imagery (building shots, floor plans, unit
/// layouts) ships bundled and is written to disk on first run. Prices, where
/// known, are representative per-unit asking prices converted from VND at
/// ~25,000 VND/USD; the under-construction towers have none yet (`None`).
/// Idempotent: a non-empty DB is left untouched.
pub async fn seed(store: &SqliteStore) -> Result<(), DomainError> {
	let count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM properties").fetch_one(&store.pool).await.map_err(map_sqlx_error)?.get("n");
	if count > 0 {
		return Ok(());
	}

	struct Seed {
		/// Fixed UUID so a property keeps the same id across reseeds — deep links and the
		/// embed's `Q1_PROPERTY` / `TMS_PROPERTY` constants depend on it being stable.
		id: &'static str,
		name: &'static str,
		place: &'static str,
		/// `None` where we have no real number yet (the two under-construction towers).
		price: Option<f64>,
		state: PropertyState,
		construction: ConstructionStatus,
		/// Headline target rate (% / yr), surfaced as "Target Yield". 0 = unset.
		target_appreciation: f64,
		developer: &'static str,
		research_url: &'static str,
		terms: &'static str,
		reasoning: &'static str,
		/// (filename-on-disk, content-type, bytes). Floor plans / unit layouts ride
		/// as `Pic` too so they render inline in the media gallery.
		pics: &'static [(&'static str, &'static str, &'static [u8])],
	}

	const JPG: &str = "image/jpeg";
	const PNG: &str = "image/png";

	// Purchase instants are relative to now so the demo holds regardless of when the
	// DB is seeded. Melody is deliberately old: its mock series ends long before today,
	// so the chart shows the dotted "stale" projection out to the present.
	let now = jiff::Timestamp::now();
	let weeks_ago = |w: i64| jiff::Timestamp::from_second(now.as_second() - w * 7 * 24 * 3600).expect("seed purchase date in range");

	let seeds = [
		Seed {
			id: "9a5a7d3b-cd42-4d65-a426-5b705a3d0cc9",
			name: "Quy Nhơn Melody",
			place: "ChIJOYTnE0ZtbzERRZnbRLfEIU8", // Căn Hộ Quy Nhơn Melody
			price: Some(96_000.0),
			state: PropertyState::Purchased(weeks_ago(54)),
			construction: ConstructionStatus::Completed,
			target_appreciation: 18.0,
			developer: "Hưng Thịnh",
			research_url: "https://www.hungthinhland.com/en/projects/detail/QUY-NHON-MELODY.html",
			terms: "Handed over early 2024 (topped out 2021, completed Dec 2023). 4-star seafront tourism-apartment standard.",
			reasoning: "Two 35-floor towers (Tropical & Flamenco) with Kim Cúc, 1,332 units + 21 shops on the An Dương Vương–Chương Dương beachfront. Representative 2-BR ≈ 2.4 tỷ VND. Beachfront short-stay rental demand behind an established national brand.",
			pics: &[("building.jpg", JPG, include_bytes!("../assets/seed/melody/building.jpg"))],
		},
		Seed {
			id: "dd5a8289-a5cd-4fb4-8e69-582b065179fc",
			name: "Vina2 Panorama Quy Nhơn",
			place: "ChIJTX0aij9rbzERvgC-Y2tqNxw", // VINA2 Panorama
			price: Some(60_000.0),
			state: PropertyState::Purchased(weeks_ago(20)),
			construction: ConstructionStatus::Completed,
			target_appreciation: 12.0,
			developer: "VINA2",
			research_url: "https://quynhonhomes.vn/can-ho-quy-nhon/can-ho-vina2-panorama/",
			terms: "Built and handed over from early 2024; residents occupying. Move-in available from 30% of unit value.",
			reasoning: "20 floors, 252 units (Studio–3BR) in the Đê Đông resettlement area, Nhơn Bình; riverside with pool and shophouse podium. ~22–26 tr/m². Lowest entry price of the four; Hà Thanh river / Thị Nại lagoon outlook.",
			pics: &[
				("building.png", PNG, include_bytes!("../assets/seed/vina2_panorama/building.png")),
				("real.jpg", JPG, include_bytes!("../assets/seed/vina2_panorama/real.jpg")),
				("floorplan.jpg", JPG, include_bytes!("../assets/seed/vina2_panorama/floorplan.jpg")),
			],
		},
		Seed {
			id: "1eeab4aa-c377-4fcb-aebd-57f917a0b844",
			name: "Ecolife Riverside Quy Nhơn",
			place: "ChIJGb0UXFRrbzER9Ym2RZ7csFE", // Ecolife Riverside Quy Nhơn
			price: Some(59_000.0),
			state: PropertyState::Purchased(weeks_ago(14)),
			construction: ConstructionStatus::Completed,
			target_appreciation: 10.0,
			developer: "Capital House",
			research_url: "https://quynhonhomes.vn/can-ho-quy-nhon/ecolife-riverside/",
			terms: "Completed and handed over; red book (sổ hồng) issued — move in immediately.",
			reasoning: "27-floor single tower, 694 units on Điện Biên Phủ St along the Hà Thanh river. Green-building positioning; issued title lowers legal risk. Representative 2-BR ≈ 1.48 tỷ VND.",
			pics: &[
				("building.png", PNG, include_bytes!("../assets/seed/ecolife/building.png")),
				("real.jpg", JPG, include_bytes!("../assets/seed/ecolife/real.jpg")),
				("floorplan.png", PNG, include_bytes!("../assets/seed/ecolife/floorplan.png")),
			],
		},
		Seed {
			id: "12958cdb-b5e1-4aa3-80dd-91a8b9048916",
			name: "The Calla (Calla Apartment Quy Nhơn)",
			place: "ChIJt5jJjCxtbzERKfYIEUv0i-A", // The Calla (matches the shared map pin)
			price: Some(80_000.0),
			state: PropertyState::Purchased(weeks_ago(9)),
			construction: ConstructionStatus::Completed,
			target_appreciation: 14.0,
			developer: "Armo",
			research_url: "https://quynhonhomes.vn/can-ho-quy-nhon/calla-apartment-quy-nhon/",
			terms: "Completed, sổ hồng available. Bank financing up to 80% LTV with interest grace through handover.",
			reasoning: "29-floor tower (100m), 454 units + 13 shophouses in the Vũng Chua green urban area (QL1D, Ghềnh Ráng); ~800m to the beach. First garden-apartment in Quy Nhơn; mountain + sea + city views. Units 39–82m² (1–3BR), ~25–28 tr/m². Total project investment 563 tỷ VND.",
			pics: &[
				("building.jpg", JPG, include_bytes!("../assets/seed/calla/building.jpg")),
				("livingroom.jpg", JPG, include_bytes!("../assets/seed/calla/livingroom.jpg")),
				("floorplan.png", PNG, include_bytes!("../assets/seed/calla/floorplan.png")),
				("unit_87m2.png", PNG, include_bytes!("../assets/seed/calla/unit_87m2.png")),
			],
		},
		Seed {
			id: "b41510ef-1e74-4d4f-a15c-1dfafdd0ee5a", // matches embed::Q1_PROPERTY
			name: "Q1 Tower (Cadia Quy Nhơn)",
			place: "ChIJDQMq0yFtbzERY32pkB70paY", // Q1 Tower Quy Nhơn, 1 Ngô Mây
			price: Some(90_000.0),                // provisional: pre-handover branded beachfront residence
			state: PropertyState::Purchasing,
			construction: ConstructionStatus::UnderConstruction,
			target_appreciation: 12.0,
			developer: "Phát Đạt",
			research_url: "https://q1-tower.vn/",
			terms: "Under construction (broke ground Jun 2022). Beachfront 5-star branded-residence; not yet handed over.",
			reasoning: "Diamond-plot 5,246m² at No.1 Ngô Mây, directly facing Quy Nhơn beach & Nguyễn Tất Thành square. 5-star tourism apartments + hotel operated to Wyndham standard, smart-home fitted. Branded-residence scarcity in the city centre; pre-handover entry.",
			pics: &[
				("building.jpg", JPG, include_bytes!("../assets/seed/q1_tower/building.jpg")),
				("render.jpg", JPG, include_bytes!("../assets/seed/q1_tower/render.jpg")),
				("livingroom.jpg", JPG, include_bytes!("../assets/seed/q1_tower/livingroom.jpg")),
			],
		},
		Seed {
			id: "c19bded1-1a13-49ad-a0f0-549b2aec2d0e", // matches embed::TMS_PROPERTY
			name: "TMS Luxury Hotel & Residence Quy Nhơn",
			place: "ChIJBVOIrolsbzERr_9ibfn1t-I", // Grand Hyams Hotel — the 5-star hotel occupying the TMS tower
			price: Some(76_000.0),                // average apartment ≈ 1.9 tỷ VND @ ~25,000 VND/USD
			state: PropertyState::Interesting,
			construction: ConstructionStatus::Completed,
			target_appreciation: 12.0,
			developer: "TMS Group",
			research_url: "https://tms-quynhon.com",
			terms: "Completed and operating since 2022 (groundbreaking 2017); sổ hồng issuance underway. Condotel lease-back: 10%/yr guaranteed for 10 years (after 95% payment) then 85% owner / 15% operator, or 80/20 non-guaranteed, plus ~15 free nights/yr — treat the guarantee as a marketed target given VN condotel payout risk.",
			reasoning: "42-floor beachfront landmark (tallest in Quy Nhơn) at 28 Nguyễn Huệ, ~240m from the city beach. Single tower: ~746 condotel/tourist apartments (F4–F28) above 328 five-star rooms run as Grand Hyams Hotel (F29–F41) and an F41–42 sky bar. 1BR ~45–50m², 2BR ~65–71m² at ~29–36 tr/m²; representative apartment ≈ 1.9 tỷ VND (~$76k). Operating asset with branded-residence scarcity in the city centre.",
			pics: &[("building.jpg", JPG, include_bytes!("../assets/seed/tms/building.jpg"))],
		},
		Seed {
			id: "9c4acfee-9597-455e-b983-b60143fdaa90",
			name: "Triton — Quy Nhơn Sky Residence",
			place: "ChIJ7QjTJQBtbzERetFnxYHlsUM", // Triton Sky Residence, 72B Tây Sơn
			price: Some(72_000.0),                // provisional: pricing still firming up (launched 2025)
			state: PropertyState::Purchasing,
			construction: ConstructionStatus::UnderConstruction,
			target_appreciation: 0.0,
			developer: "Arita",
			research_url: "https://tritonquynhonsky.vn/",
			terms: "Under construction (launched 2025); pricing still being released. Not yet handed over.",
			reasoning: "48-storey premium tower (4 basements) at 72B Tây Sơn, Quy Nhơn Nam. Tallest of the set; early-stage entry on a central arterial. Specs and pricing still firming up — treat valuation as provisional.",
			pics: &[
				("building.jpg", JPG, include_bytes!("../assets/seed/triton/building.jpg")),
				("location.jpg", JPG, include_bytes!("../assets/seed/triton/location.jpg")),
				("floorplan.jpg", JPG, include_bytes!("../assets/seed/triton/floorplan.jpg")),
			],
		},
	];

	// Developers first: the FK on properties.developer requires them to exist.
	let developers: [(&str, &str, Option<&str>); 7] = [
		(
			"TMS Group",
			"Hà Nội–based diversified group (est. 2004; Công ty Cổ phần Tập đoàn TMS) spanning real estate, hospitality and trading. TMS Quy Nhơn is its flagship Bình Định tower.",
			Some("https://tms-quynhon.com"),
		),
		(
			"Hưng Thịnh",
			"Large national developer; ecosystem includes Hưng Thịnh Land & Incons. 2022–23 liquidity stress — watch counterparty risk.",
			Some("https://hungthinhcorp.com.vn/"),
		),
		(
			"VINA2",
			"Listed contractor-developer (HNX: VC2) that builds its own projects. Smaller balance sheet.",
			Some("https://vina2.com.vn/"),
		),
		("Capital House", "Hà Nội green-building specialist behind the Ecolife / Ecohome brands.", None),
		("Armo", "Local Bình Định developer; The Calla is its flagship tower.", None),
		(
			"Phát Đạt",
			"Major listed developer (HOSE: PDR), HCMC-centric. Bình Định pipeline slipped to ~2027.",
			Some("https://phatdat.com.vn/"),
		),
		("Arita", "Local developer; Triton is early-stage with a limited track record.", None),
	];
	for (name, note, page) in developers {
		sqlx::query("INSERT INTO developers (name, note, page) VALUES (?, ?, ?)")
			.bind(name)
			.bind(note)
			.bind(page)
			.execute(&store.pool)
			.await
			.map_err(map_sqlx_error)?;
	}

	for s in seeds {
		let id = crate::domain::parse_building_id(s.id)?;
		let price = s.price.map(|p| Money::parse(p).expect("seed price is non-negative finite"));
		store
			.put(&Building {
				id,
				name: s.name.into(),
				place: GooglePlace::parse(s.place.into())?,
				construction: s.construction,
				target_appreciation: s.target_appreciation,
				developer: Some(s.developer.into()),
				research_url: ResearchUrl::parse(s.research_url.into())?,
				terms: Some(s.terms.into()),
				deal: None,
				loan: None,
				additional_reasoning: Some(s.reasoning.into()),
				apartments: mock_apartments(id, s.state, price),
				coords: None,
			})
			.await?;

		for (filename, content_type, bytes) in s.pics {
			store
				.add_file(
					PropertyFile {
						id: FileId::new(),
						building_id: id,
						apt: None,
						kind: FileKind::Pic,
						filename: (*filename).into(),
						content_type: (*content_type).into(),
					},
					bytes,
				)
				.await?;
		}
	}

	Ok(())
}

/// Self-extinguishing bridge from the files-on-disk era: fill the blob rows the
/// 0002 migration left as `x''` from `<db dir>/properties/…`, adopt the layout
/// json into `layouts`, then rename both to `*.imported`. Any missing/unreadable
/// source file is a hard error — no partial imports. `place.json` caches are
/// dropped, not imported (the cache regenerates). Delete this fn (and its `main`
/// call) once prod has run it.
pub async fn import_legacy(store: &SqliteStore, db_path: &Path) -> Result<(), DomainError> {
	let dir = db_path.parent().expect("open() created the db dir");
	let data_dir = dir.join("properties");
	let layout_path = dir.join("dashboard_layout.json");

	let pending = sqlx::query_as::<_, FileRow>("SELECT id, property_id, apt, kind, filename, content_type FROM property_files WHERE content = x''")
		.fetch_all(&store.pool)
		.await
		.map_err(map_sqlx_error)?;
	if !data_dir.exists() && !layout_path.exists() {
		// A blob-era DB next to no legacy dir is the normal end state — but rows
		// still carrying the migration placeholder mean their bytes are GONE.
		if !pending.is_empty() {
			return Err(DomainError::Repository(format!(
				"{} property_files rows have no content and the legacy data dir {} is gone — restore it and reboot",
				pending.len(),
				data_dir.display()
			)));
		}
		return Ok(());
	}

	for row in pending {
		let f = PropertyFile::try_from(row)?;
		let mut d = data_dir.join(f.building_id.raw().to_string());
		if let Some(n) = f.apt {
			d = d.join("apt").join(n.to_string());
		}
		let path = d.join(format!("{}__{}", f.id.raw(), f.filename));
		let bytes = std::fs::read(&path).map_err(|e| DomainError::Repository(format!("legacy import: read {}: {e}", path.display())))?;
		sqlx::query("UPDATE property_files SET content = ? WHERE id = ?")
			.bind(&bytes)
			.bind(f.id.raw().to_string())
			.execute(&store.pool)
			.await
			.map_err(map_sqlx_error)?;
	}

	match std::fs::read_to_string(&layout_path) {
		Ok(s) =>
			for (k, v) in crate::api::parse_seeds(&s).map_err(DomainError::Repository)? {
				store.save_layout("", &k, &v.to_string()).await?;
			},
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {} // layout is only written after a save; absent is a valid legacy state
		Err(e) => return Err(DomainError::Repository(format!("legacy import: read {}: {e}", layout_path.display()))),
	}

	for p in [&data_dir, &layout_path] {
		if p.exists() {
			let to = PathBuf::from(format!("{}.imported", p.display()));
			std::fs::rename(p, &to).map_err(|e| DomainError::Repository(format!("legacy import: rename {} → {}: {e}", p.display(), to.display())))?;
		}
	}
	dioxus::logger::tracing::info!("legacy data imported into sqlite; originals renamed to *.imported");
	Ok(())
}

/// Synthesise a stable lot roster for a building from its id, so the two-level model
/// is exercised end to end while a real per-lot source does not exist. `our_state` and
/// `base` are the building's representative single-unit figures: a handful of lots take
/// our relationship, the rest split Sold/Available, and prices jitter around `base`.
/// Called once at seed time — the result is persisted as real lots, not re-synthesised
/// per read. Logged once so the data is never silently taken as real.
fn mock_apartments(id: BuildingId, our_state: PropertyState, base: Option<Money>) -> Vec<Apartment> {
	static NOTICE: std::sync::Once = std::sync::Once::new();
	NOTICE.call_once(|| {
		dioxus::logger::tracing::warn!("apartment rosters are procedurally seeded from the building id — no real per-lot source yet");
	});

	let root = id.raw().as_u64_pair().0;
	let mix = |i: u64| -> u64 {
		let mut z = root.wrapping_add(i.wrapping_mul(0x9e3779b97f4a7c15));
		z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
		z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
		z ^ (z >> 31)
	};
	let total = 20 + (mix(0) % 21) as u32; // 20..=40 lots
	let ours = 1 + (mix(1) % 5) as u32; // 1..=5 of them ours
	let ours_status = match our_state {
		PropertyState::Purchased(ts) => ApartmentStatus::Purchased(ts),
		PropertyState::Purchasing => ApartmentStatus::Purchasing,
		PropertyState::Interesting => ApartmentStatus::Interesting,
	};
	(1..=total)
		.map(|n| {
			let status = if n <= ours {
				ours_status
			} else if mix(n as u64 * 3) % 100 < 55 {
				ApartmentStatus::Sold
			} else {
				ApartmentStatus::Available
			};
			let price = base.map(|m| {
				let f = 0.82 + (mix(n as u64 * 7) % 37) as f64 / 100.0; // 0.82..=1.18
				Money::parse((m.amount() * f).round()).expect("jittered price stays non-negative finite")
			});
			Apartment {
				number: n,
				status,
				price,
				price_series: Vec::new(),
			}
		})
		.collect()
}

#[derive(FromRow)]
struct DeveloperRow {
	name: String,
	note: String,
	page: Option<String>,
}

#[derive(FromRow)]
struct FileRow {
	id: String,
	property_id: String,
	apt: Option<i64>,
	kind: String,
	filename: String,
	content_type: String,
}

impl TryFrom<FileRow> for PropertyFile {
	type Error = DomainError;

	fn try_from(row: FileRow) -> Result<Self, Self::Error> {
		Ok(Self {
			id: crate::domain::parse_file_id(&row.id).map_err(corrupt_row)?,
			building_id: crate::domain::parse_building_id(&row.property_id).map_err(corrupt_row)?,
			apt: row.apt.map(|n| n as u32),
			kind: row.kind.parse().map_err(|e| corrupt_row(DomainError::Repository(format!("unknown file kind: {e}"))))?,
			filename: row.filename,
			content_type: row.content_type,
		})
	}
}

impl Repository for SqliteStore {
	type Aggregate = Building;
}

impl Reader for SqliteStore {
	type Aggregate = Building;
}

#[async_trait]
impl BuildingRepository for SqliteStore {
	/// Rows are loaded then filtered in Rust via `spec.holds`. The spec engine is
	/// in-memory by design; SQL pushdown is explicitly descoped.
	async fn list(&self, spec: Option<&(dyn Specification<Building> + Sync)>) -> Result<Vec<Building>, DomainError> {
		let rows = sqlx::query("SELECT doc FROM properties").fetch_all(&self.pool).await.map_err(map_sqlx_error)?;
		let mut out = Vec::new();
		for row in rows {
			let b = deserialize_building(row.get("doc"))?;
			if spec.map(|s| Specification::holds(s, &b)).unwrap_or(true) {
				out.push(b);
			}
		}
		Ok(out)
	}

	async fn get(&self, id: BuildingId) -> Result<Option<Building>, DomainError> {
		let row = sqlx::query("SELECT doc FROM properties WHERE id = ?")
			.bind(id.raw().to_string())
			.fetch_optional(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		row.map(|r| deserialize_building(r.get("doc"))).transpose()
	}

	async fn put(&self, b: &Building) -> Result<(), DomainError> {
		if let Some(dev) = &b.developer
			&& self.get_developer(dev).await?.is_none()
		{
			return Err(DomainError::Validation(format!("unknown developer: {dev}")));
		}
		let doc = serde_json::to_string(b).map_err(|e| DomainError::Repository(format!("serialize building: {e}")))?;
		sqlx::query("INSERT INTO properties (id, doc) VALUES (?, ?)")
			.bind(b.id.raw().to_string())
			.bind(doc)
			.execute(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		Ok(())
	}

	async fn get_developer(&self, name: &str) -> Result<Option<Developer>, DomainError> {
		let row = sqlx::query_as::<_, DeveloperRow>("SELECT name, note, page FROM developers WHERE name = ?")
			.bind(name)
			.fetch_optional(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		Ok(row.map(|r| Developer {
			name: r.name,
			note: r.note,
			page: r.page,
		}))
	}

	async fn add_file(&self, f: PropertyFile, content: &[u8]) -> Result<(), DomainError> {
		sqlx::query("INSERT INTO property_files (id, property_id, apt, kind, filename, content_type, content) VALUES (?, ?, ?, ?, ?, ?, ?)")
			.bind(f.id.raw().to_string())
			.bind(f.building_id.raw().to_string())
			.bind(f.apt.map(|n| n as i64))
			.bind(f.kind.as_ref())
			.bind(&f.filename)
			.bind(&f.content_type)
			.bind(content)
			.execute(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		Ok(())
	}

	async fn file_content(&self, id: FileId) -> Result<Vec<u8>, DomainError> {
		let row = sqlx::query("SELECT content FROM property_files WHERE id = ?")
			.bind(id.raw().to_string())
			.fetch_optional(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		match row {
			Some(r) => Ok(r.get("content")),
			None => Err(DomainError::NotFound {
				entity: "file",
				id: id.raw().to_string(),
			}),
		}
	}

	/// All files for a building, lot-level and building-level alike; the caller
	/// (`api::list_files`) narrows to the active level via each file's `apt`.
	async fn list_files(&self, id: BuildingId) -> Result<Vec<PropertyFile>, DomainError> {
		let rows = sqlx::query_as::<_, FileRow>("SELECT id, property_id, apt, kind, filename, content_type FROM property_files WHERE property_id = ?")
			.bind(id.raw().to_string())
			.fetch_all(&self.pool)
			.await
			.map_err(map_sqlx_error)?;
		rows.into_iter().map(PropertyFile::try_from).collect()
	}
}

/// Reconstruct a building from its stored document. The aggregate was serialised from a
/// valid value on the way in, so a parse failure means the row is corrupt — a repository
/// error, never a client-facing validation, and never silently defaulted away.
fn deserialize_building(doc: &str) -> Result<Building, DomainError> {
	serde_json::from_str(doc).map_err(|e| corrupt_row(DomainError::Repository(format!("deserialize building: {e}"))))
}

/// A persisted row failed domain re-validation: the database holds bad data.
fn corrupt_row(err: DomainError) -> DomainError {
	DomainError::Repository(format!("corrupt property row: {err}"))
}

/// Translate sqlx errors into domain errors. A unique violation is an honest
/// `Conflict`; everything else is an opaque `Repository`. A database failure is
/// never reported as a `Validation`.
fn map_sqlx_error(err: sqlx::Error) -> DomainError {
	if let sqlx::Error::Database(ref db_err) = err
		&& db_err.is_unique_violation()
	{
		return DomainError::Conflict("duplicate primary key".into());
	}
	DomainError::Repository(err.to_string())
}
