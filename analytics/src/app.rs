use dioxus::prelude::*;
use ev_lib::t;

use crate::{
	dashboard::Dashboard,
	domain::{Building, BuildingId, PropertyStateKind},
	i18n::use_t,
};

/// The selected building, shared from the root so all panels read/write the same
/// selection. `None` until the user clicks a marker / heatmap tile.
pub type SelectedBuilding = Signal<Option<BuildingId>>;

/// The selected apartment *within* the selected building, by its 1-based `number`.
/// `Some` ⇒ apartment view, `None` ⇒ building view. Only meaningful when a building
/// is selected.
pub type SelectedAppt = Signal<Option<u32>>;

/// Portfolio state filter, shared so the map and heatmap show the same set.
/// Defaults to `Purchased`.
pub type Filter = Signal<Vec<PropertyStateKind>>;

/// The fetched record for the current selection, resolved once at the root and
/// shared so the top bar, chart, and details panel don't each re-fetch it.
/// Outer `None` = still loading; `Some(None)` = nothing selected.
pub type BuildingResource = Resource<Option<Building>>;

/// Band the dashboard currently has a layout for, written on every band entry and
/// surfaced in [`BuildTag`]. `None` until the first measure.
pub type SeedGroup = Signal<Option<tilers::core::Band>>;

/// The dashboard, mounted by the `analytics_dashboard` page bundle into
/// `<mfe-analytics-dashboard>`. The host owns the document: fonts, tokens, preflight
/// and the `Translator` (`mfe!`) all come from above.
#[component]
pub fn App() -> Element {
	let tr = use_t();
	let deep = use_hook(DeepLink::capture);
	let selected: SelectedBuilding = use_signal(|| deep.building);
	use_context_provider(|| selected);

	let mut appt: SelectedAppt = use_signal(|| deep.appt);
	use_context_provider(|| appt);

	let filter: Filter = use_signal(|| deep.filter.clone());
	use_context_provider(|| filter);

	let seed_group: SeedGroup = use_signal(|| None);
	use_context_provider(|| seed_group);

	// Mirror the state filter into `?selection=` so a shared link restores the set.
	#[cfg(target_arch = "wasm32")]
	use_effect(move || {
		let csv = filter().iter().map(AsRef::as_ref).collect::<Vec<&str>>().join(",");
		crate::map::sync_selection(&csv);
	});

	let building: BuildingResource = use_resource(move || async move {
		match selected() {
			Some(id) => crate::api::get_building(id).await.ok().flatten(),
			None => None,
		}
	});
	use_context_provider(|| building);

	// Validate `appt` against the resolved building: a deep-linked or stale index that
	// names no lot drops to the building view rather than keeping a tainted selection.
	use_effect(move || {
		let Some(n) = appt() else { return };
		let guard = building.read();
		let Some(Some(b)) = guard.as_ref() else { return }; // still loading
		let known = b.apartments.iter().any(|a| a.number == n);
		drop(guard);
		if !known {
			dioxus::logger::tracing::warn!(appt = n, "appt out of range for building — dropping to building view");
			appt.set(None);
		}
	});

	// One global ArrowLeft/ArrowRight handler, installed once for the page lifetime;
	// it no-ops unless an apartment is selected. Mirrors the map's marker-click bridge.
	#[cfg(target_arch = "wasm32")]
	use_hook(move || {
		let cb = wasm_bindgen::closure::Closure::<dyn FnMut(i32)>::new(move |dir: i32| {
			if appt().is_some() {
				cycle_appt(building, appt, dir);
			}
		});
		crate::map::on_keynav(&cb);
		cb.forget();
	});

	// Maps JS key is server-side config; fetch it so the loader `<script>` can be
	// emitted with the right key (Maps JS keys are public, restricted by referrer).
	let maps_key = use_resource(crate::api::maps_api_key);
	let maps_src = match &*maps_key.read() {
		Some(Ok(key)) if !key.is_empty() => Some(format!("https://maps.googleapis.com/maps/api/js?key={key}&libraries=places&v=weekly&callback=__reaMapsReady")),
		Some(Ok(_)) => {
			dioxus::logger::tracing::error!("maps_api_key resolved empty — set it in config; the map will not load");
			None
		}
		Some(Err(e)) => {
			dioxus::logger::tracing::error!(%e, "maps_api_key fetch failed — the map will not load");
			None
		}
		None => None, // still loading
	};

	// The dock overlay positions panels from live DOM measurements and writes dynamic
	// `style` attributes; under SSR hydration that mismatches the server DOM and the
	// Dioxus interpreter aborts the mutation batch — panels stay `visibility:hidden`,
	// blank and click-eating. An interactive layout has nothing to pre-render, so we
	// render it client-only (effects don't run during SSR, so `ready` flips post-mount).
	let mut ready = use_signal(|| false);
	use_effect(move || ready.set(true));

	rsx! {
		// Chromeless: the host owns the surrounding nav — we render only our own
		// dashboard, filling the viewport we're given.
		main {
			if ready() {
				if let Some(src) = maps_src {
					document::Script { src, defer: true }
				}
				document::Script { src: "https://cdn.plot.ly/plotly-basic-2.35.2.min.js", defer: true }
				Dashboard {}
				BuildTag {}
				// Discord-style prev/next, only while viewing an apartment; wraps around.
				if appt().is_some() {
					button {
						class: "fixed left-2 top-1/2 z-40 -translate-y-1/2 flex h-12 w-12 items-center justify-center rounded-full border border-border bg-main-black/70 text-2xl text-main-mist transition hover:text-white",
						"aria-label": t!(tr, "app.prevApartment", "Previous apartment"),
						onclick: move |_| cycle_appt(building, appt, -1),
						"‹"
					}
					button {
						class: "fixed right-2 top-1/2 z-40 -translate-y-1/2 flex h-12 w-12 items-center justify-center rounded-full border border-border bg-main-black/70 text-2xl text-main-mist transition hover:text-white",
						"aria-label": t!(tr, "app.nextApartment", "Next apartment"),
						onclick: move |_| cycle_appt(building, appt, 1),
						"›"
					}
				}
			}
		}
	}
}
/// Deep-link state read from the host URL once at startup.
#[derive(Clone)]
struct DeepLink {
	building: Option<BuildingId>,
	appt: Option<u32>,
	filter: Vec<PropertyStateKind>,
}

impl DeepLink {
	fn capture() -> Self {
		#[cfg(target_arch = "wasm32")]
		{
			// URL is user-editable: silently drop tokens that don't name a state, and
			// fall back to the default set if nothing valid remains.
			let filter: Vec<_> = crate::map::url_selection().split(',').filter_map(|s| s.trim().parse().ok()).collect();
			Self {
				building: crate::domain::parse_building_id(&crate::map::url_building()).ok(),
				appt: crate::map::url_appt().parse().ok(),
				filter: if filter.is_empty() { vec![PropertyStateKind::Purchased] } else { filter },
			}
		}
		#[cfg(not(target_arch = "wasm32"))]
		Self {
			building: None,
			appt: None,
			filter: vec![PropertyStateKind::Purchased],
		}
	}
}

/// Inconspicuous deployed-version tag pinned to the bottom-right (plus the seed group
/// the dock is on), linking to the exact commit on GitHub so we can tell what's live
/// at a glance. Rev resolution lives in [`crate::chrome::build_rev`].
#[component]
fn BuildTag() -> Element {
	let hash = crate::chrome::build_rev();
	let seed_group = use_context::<SeedGroup>();
	rsx! {
		a {
			href: "https://github.com/EV-Estate/analytics/commit/{hash}",
			target: "_blank",
			rel: "noopener noreferrer",
			class: "fixed bottom-1 right-2 z-10 font-mono text-[10px] text-muted-foreground/25 transition-colors hover:text-muted-foreground/70",
			if let Some(g) = seed_group() {
				"{g}·"
			}
			"v{env!(\"CARGO_PKG_VERSION\")}·{hash}"
		}
	}
}

/// Step `SelectedAppt` to the next/previous lot of the current building (wrap-around).
/// A no-op until the building has resolved.
fn cycle_appt(building: BuildingResource, mut appt: SelectedAppt, dir: i32) {
	let guard = building.read();
	let Some(Some(b)) = guard.as_ref() else { return };
	let nums: Vec<u32> = b.apartments.iter().map(|a| a.number).collect();
	if nums.is_empty() {
		return;
	}
	let len = nums.len() as i32;
	let idx = appt().and_then(|n| nums.iter().position(|x| *x == n)).unwrap_or(0) as i32;
	let next = nums[(((idx + dir) % len + len) % len) as usize];
	drop(guard);
	appt.set(Some(next));
}
