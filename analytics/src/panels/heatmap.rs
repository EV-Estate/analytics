use dioxus::prelude::*;
use ev_lib::{
	t,
	uikit::{Card, CardContent, Skeleton},
};

use crate::{
	app::{Filter, SelectedAppt, SelectedBuilding},
	domain::{Apartment, ApartmentStatus, Building, BuildingId, PropertyStateKind},
	i18n::use_t,
};

/// Portfolio heatmap + drill-down: a treemap of buildings (area ∝ the summed price of
/// the lots that match the active filter, colour ∝ their mean value change). The
/// selected building is outlined and subdivided into its own apartments — clicking a
/// sub-tile descends into that apartment.
#[component]
pub fn PortfolioHeatmap() -> Element {
	let tr = use_t();
	let filter = use_context::<Filter>();
	// The heatmap is a portfolio view: `Interesting` is never a holding, so it stays out
	// even when the user has it selected.
	let states = use_memo(move || filter().into_iter().filter(|k| *k != PropertyStateKind::Interesting).collect::<Vec<_>>());
	let buildings = use_resource(move || {
		let states = states();
		async move { crate::api::list_buildings(Some(states)).await.unwrap_or_default() }
	});

	rsx! {
		// h-full so the treemap fills (and reflows with) its dock pane; overflow-hidden so the
		// flex-1 body shrinks to the pane instead of spilling its last row past the bottom.
		Card { class: "flex h-full flex-col overflow-hidden",
			CardContent { class: "relative flex-1",
				match &*buildings.read() {
					None => rsx! { Skeleton { class: "h-full w-full" } },
					Some(list) if list.is_empty() => rsx! {
						div { class: "flex h-full items-center justify-center text-sm text-muted-foreground",
							{t!(tr, "heatmap.empty", "No holdings to display.")}
						}
					},
					Some(list) => rsx! { Treemap { buildings: list.clone(), states: states() } },
				}
			}
		}
	}
}

/// One building tile: geometry + paint + the lots needed to subdivide it when selected.
#[derive(Clone, PartialEq)]
struct BTile {
	id: BuildingId,
	name: String,
	rect: Rect,
	change: f64,
	/// No selected lot is actually owned (`Purchased`) — a not-yet-ours holding, drawn
	/// provisional (dimmed + hatched) just like a `Purchasing` apartment sub-tile.
	prospect: bool,
	apartments: Vec<Apartment>,
}

/// A lot's contribution to area: its price, or the mean known price as a fallback so a
/// price-less lot stays visible rather than collapsing to a sliver.
fn lot_value(a: &Apartment, fallback: f64) -> f64 {
	a.price.map_or(fallback, |m| m.amount()).max(1.0)
}

/// The lots that "made the selection": ours in one of the active filter kinds.
fn selected_lots<'a>(b: &'a Building, states: &'a [PropertyStateKind]) -> impl Iterator<Item = &'a Apartment> {
	b.apartments.iter().filter(move |a| a.status.portfolio_kind().is_some_and(|k| states.contains(&k)))
}

fn layout(buildings: &[Building], states: &[PropertyStateKind]) -> Vec<BTile> {
	let known: Vec<f64> = buildings
		.iter()
		.flat_map(|b| &b.apartments)
		.filter_map(|a| a.price.map(|m| m.amount()))
		.filter(|a| *a > 0.0)
		.collect();
	let fallback = if known.is_empty() { 1.0 } else { known.iter().sum::<f64>() / known.len() as f64 };

	let values: Vec<f64> = buildings.iter().map(|b| selected_lots(b, states).map(|a| lot_value(a, fallback)).sum::<f64>().max(1.0)).collect();

	// Owned buildings outrank prospects, so they always take the left slab and the
	// Purchasing-only ones the slab to their right — never intermixed by size. Each slab
	// is squarified on its own and widthed by its share of total value.
	let prospect: Vec<bool> = buildings
		.iter()
		.map(|b| !selected_lots(b, states).any(|a| matches!(a.status, ApartmentStatus::Purchased(_))))
		.collect();
	let owned_idx: Vec<usize> = (0..buildings.len()).filter(|&i| !prospect[i]).collect();
	let prospect_idx: Vec<usize> = (0..buildings.len()).filter(|&i| prospect[i]).collect();
	let total = values.iter().sum::<f64>().max(1.0);
	let owned_w = 100.0 * owned_idx.iter().map(|&i| values[i]).sum::<f64>() / total;

	let mut rects = vec![Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }; buildings.len()];
	for (idxs, slab) in [
		(
			&owned_idx,
			Rect {
				x: 0.0,
				y: 0.0,
				w: owned_w,
				h: 100.0,
			},
		),
		(
			&prospect_idx,
			Rect {
				x: owned_w,
				y: 0.0,
				w: 100.0 - owned_w,
				h: 100.0,
			},
		),
	] {
		let vals: Vec<f64> = idxs.iter().map(|&i| values[i]).collect();
		for (k, r) in squarify(&vals, slab).into_iter().enumerate() {
			rects[idxs[k]] = r;
		}
	}

	buildings
		.iter()
		.enumerate()
		.map(|(i, b)| {
			let sel: Vec<&Apartment> = selected_lots(b, states).collect();
			let change = if sel.is_empty() {
				0.0
			} else {
				sel.iter().map(|a| apt_change(b.id, a.number)).sum::<f64>() / sel.len() as f64
			};
			BTile {
				id: b.id,
				name: b.name.clone(),
				rect: rects[i],
				change,
				prospect: prospect[i],
				apartments: b.apartments.clone(),
			}
		})
		.collect()
}

#[component]
fn Treemap(buildings: Vec<Building>, states: Vec<PropertyStateKind>) -> Element {
	let mut selected = use_context::<SelectedBuilding>();
	let mut appt = use_context::<SelectedAppt>();
	let tiles = layout(&buildings, &states);

	rsx! {
		div { class: "relative h-full w-full overflow-hidden rounded-lg bg-main-surface",
			for t in tiles {
				if selected() == Some(t.id) {
					BuildingCell { tile: t }
				} else {
					{
						let BTile { id, name, rect: r, change, prospect, .. } = t;
						let (dim, stripes) = crate::uikit::provisional(prospect);
						rsx! {
							button {
								key: "{id}",
								class: "absolute flex flex-col justify-between overflow-hidden rounded-md p-2.5 text-left transition-[filter] hover:brightness-110 {dim}",
								style: "left:calc({r.x:.4}% + 2px);top:calc({r.y:.4}% + 2px);width:calc({r.w:.4}% - 4px);height:calc({r.h:.4}% - 4px);background-color:{heat_color(change)}{stripes}",
								onclick: move |_| {
									selected.set(Some(id));
									appt.set(None);
								},
								div { class: "flex min-w-0 flex-col gap-0.5",
									span { class: "truncate text-sm font-semibold text-white", "{name}" }
									span { class: "text-xs text-white/80", "{change:+.1}%" }
								}
							}
						}
					}
				}
			}
		}
	}
}

/// The selected building: an accent-t3 outline subdivided into its apartments. Each
/// sub-tile descends into that apartment; the active one keeps the selection ring.
#[component]
fn BuildingCell(tile: BTile) -> Element {
	let tr = use_t();
	let mut selected = use_context::<SelectedBuilding>();
	let mut appt = use_context::<SelectedAppt>();
	// `prospect` is a building-level flag; the expanded cell marks each lot individually.
	let BTile {
		id,
		name,
		rect: r,
		change,
		apartments,
		..
	} = tile;

	let known: Vec<f64> = apartments.iter().filter_map(|a| a.price.map(|m| m.amount())).filter(|a| *a > 0.0).collect();
	let fallback = if known.is_empty() { 1.0 } else { known.iter().sum::<f64>() / known.len() as f64 };
	let values: Vec<f64> = apartments.iter().map(|a| lot_value(a, fallback)).collect();
	let inner = squarify(&values, Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 });

	rsx! {
		div {
			key: "{id}",
			class: "absolute z-20 overflow-hidden rounded-md ring-2 ring-main-accent-t3",
			style: "left:calc({r.x:.4}% + 2px);top:calc({r.y:.4}% + 2px);width:calc({r.w:.4}% - 4px);height:calc({r.h:.4}% - 4px);background-color:{heat_color(change)}",
			span { class: "pointer-events-none absolute left-1 top-1 z-30 rounded bg-main-black/70 px-1.5 py-0.5 text-[10px] font-semibold text-main-accent-t3", "{name}" }
			for (a, ar) in apartments.into_iter().zip(inner) {
				{
					let n = a.number;
					let ch = apt_change(id, n);
					let is_sel = appt() == Some(n);
					let ring = if is_sel { "ring-2 ring-main-accent-t1 z-10" } else { "" };
					let (dim, stripes) = crate::uikit::provisional(a.status == ApartmentStatus::Purchasing);
					rsx! {
						button {
							key: "{n}",
							title: t!(tr, "header.apt", "Apt {n}", n = n),
							class: "absolute overflow-hidden rounded-sm transition-[filter] hover:brightness-110 {ring} {dim}",
							style: "left:calc({ar.x:.4}% + 1px);top:calc({ar.y:.4}% + 1px);width:calc({ar.w:.4}% - 2px);height:calc({ar.h:.4}% - 2px);background-color:{heat_color(ch)}{stripes}",
							onclick: move |_| {
								selected.set(Some(id));
								appt.set(Some(n));
							},
						}
					}
				}
			}
		}
	}
}

/// No real performance data lives in the domain — `price_series` is itself a mock —
/// so derive a STABLE pseudo-change per lot from the building id + lot number. Same lot
/// always paints the same colour; range ≈ [-4%, +6%].
fn apt_change(building: BuildingId, number: u32) -> f64 {
	let seed = building.raw().as_u64_pair().0 ^ (number as u64).wrapping_mul(0x9e3779b97f4a7c15);
	((seed % 1000) as f64 / 1000.0) * 10.0 - 4.0
}

fn heat_color(change: f64) -> String {
	if change >= 0.0 {
		let t = (change / 6.0).clamp(0.0, 1.0);
		lerp_rgb((0x1f, 0x4d, 0x33), (0x1f, 0x8a, 0x4c), t)
	} else {
		let t = (-change / 4.0).clamp(0.0, 1.0);
		lerp_rgb((0x4a, 0x2b, 0x2c), (0x9e, 0x2f, 0x2c), t)
	}
}

fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> String {
	let c = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
	format!("rgb({},{},{})", c(a.0, b.0), c(a.1, b.1), c(a.2, b.2))
}

#[derive(Clone, Copy, PartialEq)]
struct Rect {
	x: f64,
	y: f64,
	w: f64,
	h: f64,
}

/// Largest aspect ratio in a row, given the side it's laid along. Lower is squarer.
fn worst(row: &[f64], side: f64) -> f64 {
	let sum: f64 = row.iter().sum();
	if sum <= 0.0 {
		return f64::INFINITY;
	}
	let max = row.iter().copied().fold(0.0_f64, f64::max);
	let min = row.iter().copied().fold(f64::INFINITY, f64::min);
	let s2 = sum * sum;
	let w2 = side * side;
	(w2 * max / s2).max(s2 / (w2 * min))
}

/// Place a finished row along the shorter side of `free`, then shrink `free`.
fn layout_row(row: &[(usize, f64)], free: &mut Rect, out: &mut [Rect]) {
	let sum: f64 = row.iter().map(|&(_, a)| a).sum();
	if free.w >= free.h {
		let w = sum / free.h;
		let mut y = free.y;
		for &(i, a) in row {
			let h = a / w;
			out[i] = Rect { x: free.x, y, w, h };
			y += h;
		}
		free.x += w;
		free.w -= w;
	} else {
		let h = sum / free.w;
		let mut x = free.x;
		for &(i, a) in row {
			let w = a / h;
			out[i] = Rect { x, y: free.y, w, h };
			x += w;
		}
		free.y += h;
		free.h -= h;
	}
}

/// Squarified treemap (Bruls–Huizing–van Wijk): partition `bounds` into one rect
/// per value, area ∝ value, keeping rects close to square. Output is in input
/// order; zero/empty input yields zero-area rects.
fn squarify(values: &[f64], bounds: Rect) -> Vec<Rect> {
	let mut out = vec![
		Rect {
			x: bounds.x,
			y: bounds.y,
			w: 0.0,
			h: 0.0
		};
		values.len()
	];
	let total: f64 = values.iter().sum();
	if total <= 0.0 {
		return out;
	}
	let scale = (bounds.w * bounds.h) / total;
	let mut order: Vec<usize> = (0..values.len()).collect();
	order.sort_by(|&a, &b| values[b].partial_cmp(&values[a]).unwrap_or(std::cmp::Ordering::Equal));

	let mut free = bounds;
	let mut row: Vec<(usize, f64)> = Vec::new();
	let mut areas: Vec<f64> = Vec::new();
	for &idx in &order {
		let area = values[idx] * scale;
		let side = free.w.min(free.h);
		let mut trial = areas.clone();
		trial.push(area);
		if row.is_empty() || worst(&trial, side) <= worst(&areas, side) {
			row.push((idx, area));
			areas.push(area);
		} else {
			layout_row(&row, &mut free, &mut out);
			row.clear();
			areas.clear();
			row.push((idx, area));
			areas.push(area);
		}
	}
	if !row.is_empty() {
		layout_row(&row, &mut free, &mut out);
	}
	out
}

#[cfg(test)]
mod tests {
	use jiff::Timestamp;
	use uuid::Uuid;

	use super::*;
	use crate::domain::{ConstructionStatus, GooglePlace, Money, ResearchUrl};

	fn apt(number: u32, status: ApartmentStatus, price: Option<f64>) -> Apartment {
		Apartment {
			number,
			status,
			price: price.map(|p| Money::parse(p).unwrap()),
			price_series: vec![],
		}
	}

	// `apt_change` keys off the id's high 64 bits → seed those for stable heat colours.
	fn building(seed: u64, name: &str, apartments: Vec<Apartment>) -> Building {
		Building {
			id: BuildingId::from_raw(Uuid::from_u128((seed as u128) << 64)),
			name: name.into(),
			place: GooglePlace::parse("place".into()).unwrap(),
			construction: ConstructionStatus::Completed,
			target_appreciation: 0.0,
			developer: None,
			research_url: ResearchUrl::parse("https://x.test".into()).unwrap(),
			terms: None,
			deal: None,
			loan: None,
			additional_reasoning: None,
			apartments,
			coords: None,
		}
	}

	fn book() -> Vec<Building> {
		let bought: Timestamp = "2024-01-01T00:00:00Z".parse().unwrap();
		vec![
			building(
				250,
				"Melody",
				vec![
					apt(1, ApartmentStatus::Purchased(bought), Some(96000.0)),
					apt(2, ApartmentStatus::Purchased(bought), Some(90000.0)),
					apt(3, ApartmentStatus::Sold, Some(80000.0)),
					apt(4, ApartmentStatus::Available, Some(85000.0)),
				],
			),
			building(
				700,
				"Vina2",
				vec![apt(1, ApartmentStatus::Purchased(bought), Some(60000.0)), apt(2, ApartmentStatus::Sold, Some(55000.0))],
			),
			building(
				900,
				"Q1",
				vec![apt(1, ApartmentStatus::Purchasing, Some(40000.0)), apt(2, ApartmentStatus::Available, Some(42000.0))],
			),
		]
	}

	fn render(buildings: Vec<Building>, states: Vec<PropertyStateKind>) -> String {
		#[component]
		fn Harness(buildings: Vec<Building>, states: Vec<PropertyStateKind>) -> Element {
			let sb: SelectedBuilding = use_signal(|| None);
			use_context_provider(|| sb);
			let sa: SelectedAppt = use_signal(|| None);
			use_context_provider(|| sa);
			rsx! { Treemap { buildings, states } }
		}
		let mut dom = VirtualDom::new_with_props(Harness, HarnessProps { buildings, states });
		dom.rebuild_in_place();
		dioxus_ssr::render(&dom)
	}

	/// A building's area is the summed price of the lots that match the filter (its owned
	/// `Purchased` lots here), so Melody (186k owned) outweighs Vina2 (60k) ~3:1, and a
	/// building with no matching lot (Q1) is filtered out exactly as the server does it.
	#[test]
	fn building_area_proportional_to_owned_value() {
		use PropertyStateKind::*;
		let states = [Purchased];
		let included: Vec<Building> = book().into_iter().filter(|b| b.state_kinds().any(|k| states.contains(&k))).collect();
		assert_eq!(included.len(), 2, "Q1 (no purchased lot) excluded");

		let tiles = layout(&included, &states);
		let area = |name: &str| tiles.iter().find(|t| t.name == name).map(|t| t.rect.w * t.rect.h).unwrap();
		let ratio = area("Melody") / area("Vina2");
		assert!((ratio - 186000.0 / 60000.0).abs() < 0.1, "area ratio {ratio} ≈ 3.1");
		for t in &tiles {
			assert!(t.rect.w * t.rect.h > 100.0, "{} collapsed to a sliver", t.name);
		}
		insta::assert_snapshot!("heatmap_book", &render(included, states.to_vec()), @"heatmap_book");
	}

	/// A building with no owned (`Purchased`) lot in the active filter is a prospect:
	/// its tile is drawn provisional (dimmed + hatched), an owned one is solid.
	#[test]
	fn prospect_buildings_drawn_provisional() {
		use PropertyStateKind::*;
		let states = [Purchased, Purchasing];
		let included: Vec<Building> = book().into_iter().filter(|b| b.state_kinds().any(|k| states.contains(&k))).collect();
		let tiles = layout(&included, &states);

		let prospect = |name: &str| tiles.iter().find(|t| t.name == name).unwrap().prospect;
		assert!(prospect("Q1"), "Q1 (purchasing only) is a prospect");
		assert!(!prospect("Melody"), "Melody (owned lots) is not a prospect");

		let html = render(included, states.to_vec());
		assert!(html.contains("repeating-linear-gradient"), "prospect tile must be hatched");
		assert!(html.contains("opacity-50"), "prospect tile must be dimmed");
	}

	/// `Interesting` is never a holding: even with every kind selected, the portfolio
	/// heatmap drops it before fetch + layout, so an interesting-only building never tiles.
	#[test]
	fn interesting_never_tiled_even_when_selected() {
		use PropertyStateKind::*;
		let bought: Timestamp = "2024-01-01T00:00:00Z".parse().unwrap();
		let book = vec![
			building(250, "Purchased", vec![apt(1, ApartmentStatus::Purchased(bought), Some(90000.0))]),
			building(700, "Purchasing", vec![apt(1, ApartmentStatus::Purchasing, Some(60000.0))]),
			building(900, "Interesting", vec![apt(1, ApartmentStatus::Interesting, Some(40000.0))]),
		];

		let states: Vec<PropertyStateKind> = vec![Purchased, Interesting, Purchasing].into_iter().filter(|k| *k != Interesting).collect();
		let included: Vec<Building> = book.into_iter().filter(|b| b.state_kinds().any(|k| states.contains(&k))).collect();

		let html = render(included, states);
		assert!(!html.contains("Interesting"), "interesting building must not tile");
		assert!(html.contains("Purchased") && html.contains("Purchasing"), "holdings must tile");
	}

	#[test]
	fn treemap_tiles_tile_the_bounds() {
		let bounds = Rect { x: 0.0, y: 0.0, w: 100.0, h: 60.0 };
		let values = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
		let rects = squarify(&values, bounds);
		assert_eq!(rects.len(), values.len());

		// Areas sum to the bounds area (full coverage, no gaps/overlap budget).
		let area: f64 = rects.iter().map(|r| r.w * r.h).sum();
		assert!((area - bounds.w * bounds.h).abs() < 1.0, "covered {area}, want {}", bounds.w * bounds.h);

		let total: f64 = values.iter().sum();
		for (v, r) in values.iter().zip(&rects) {
			assert!(r.w >= 0.0 && r.h >= 0.0, "negative tile");
			assert!(r.x >= -1e-6 && r.y >= -1e-6, "tile escapes top-left");
			assert!(r.x + r.w <= bounds.w + 1e-3 && r.y + r.h <= bounds.h + 1e-3, "tile escapes bounds");
			// Area is proportional to value.
			let want = v / total * bounds.w * bounds.h;
			assert!((r.w * r.h - want).abs() < 1e-3, "area {} not ∝ value, want {want}", r.w * r.h);
		}
	}
}
