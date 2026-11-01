use dioxus::prelude::*;
use ev_lib::{
	t,
	uikit::{Card, CardContent, Skeleton},
};

use crate::{
	app::{BuildingResource, SelectedAppt},
	domain::Building,
	i18n::use_t,
};

/// Building-level lot breakdown: TOTAL / SOLD / AVAILABLE tiles over a donut split
/// into Sold (others) / Your Selection / Available, centred on the building's share.
#[component]
pub fn LotsPanel() -> Element {
	let tr = use_t();
	let building = use_context::<BuildingResource>();
	let appt = use_context::<SelectedAppt>();

	rsx! {
		Card { class: "flex h-full flex-col overflow-hidden",
			CardContent { class: "flex-1 overflow-hidden",
				match &*building.read() {
					Some(Some(b)) if appt().is_none() => rsx! { Lots { building: b.clone() } },
					Some(Some(_)) => rsx! { p { class: "text-sm text-muted-foreground", {t!(tr, "lots.buildingLevel", "Lot breakdown is shown at building level.")} } },
					Some(None) => rsx! { p { class: "text-sm text-muted-foreground", {t!(tr, "lots.empty", "Select a building to see its lots.")} } },
					None => rsx! { Skeleton { class: "h-full w-full" } },
				}
			}
		}
	}
}

#[component]
fn Lots(building: Building) -> Element {
	let tr = use_t();
	let total = building.lots_total();
	let sold = building.lots_sold();
	let available = building.lots_available();
	let share = building.your_share();
	let yours = (share * total as f64).round() as usize;
	let sold_others = sold.saturating_sub(yours);

	let pct = |n: usize| if total == 0 { 0.0 } else { n as f64 / total as f64 * 100.0 };
	// Clockwise from top: Sold (others), Available, then Your Selection — keeping the two
	// golds adjacent across the left so the slice you own reads against the rest sold.
	let s1 = pct(sold_others);
	let s2 = s1 + pct(available);
	let gradient = format!("conic-gradient(from 0deg, #f2c94c 0% {s1:.2}%, #2c3342 {s1:.2}% {s2:.2}%, #f6dd86 {s2:.2}% 100%)");

	// The block is laid out once at its natural size; rea_fit_scale uniformly scales it to
	// fit the pane, so every part keeps its proportion under one resize rule.
	#[cfg(target_arch = "wasm32")]
	use_effect(|| fit_scale("rea-lots-fit"));

	rsx! {
		div { id: "rea-lots-fit", class: "flex h-full w-full items-center justify-center overflow-hidden",
			div { class: "flex w-80 flex-col gap-7",
				div { class: "grid grid-cols-3 gap-3",
					Tile { label: t!(tr, "lots.total", "Total lots"), value: total, value_class: "text-white" }
					Tile { label: t!(tr, "status.sold", "Sold"), value: sold, value_class: "text-main-accent-t3" }
					Tile { label: t!(tr, "status.available", "Available"), value: available, value_class: "text-main-accent-t2" }
				}
				div { class: "flex flex-col items-center gap-6",
					div { class: "relative h-48 w-48 rounded-full", style: "background:{gradient}",
						div { class: "absolute inset-[26%] flex flex-col items-center justify-center gap-0.5 rounded-full bg-main-card",
							span { class: "font-serif text-3xl font-semibold text-main-accent-t3", "{share * 100.0:.1}%" }
							span { class: "text-[10px] uppercase tracking-widest text-muted-foreground", {t!(tr, "lots.yourShare", "Your share")} }
						}
					}
					div { class: "flex items-center gap-5 text-xs text-muted-foreground",
						Legend { color: "#f2c94c", label: t!(tr, "status.sold", "Sold") }
						Legend { color: "#f6dd86", label: t!(tr, "lots.yourSelection", "Your Selection") }
						Legend { color: "#2c3342", label: t!(tr, "status.available", "Available") }
					}
				}
			}
		}
	}
}

#[component]
fn Tile(label: String, value: usize, value_class: String) -> Element {
	rsx! {
		div { class: "flex flex-col items-center gap-1 rounded-md border border-border bg-main-surface py-3",
			span { class: "font-serif text-3xl font-semibold {value_class}", "{value}" }
			span { class: "text-[10px] uppercase tracking-widest text-muted-foreground", "{label}" }
		}
	}
}

#[component]
fn Legend(color: String, label: String) -> Element {
	rsx! {
		span { class: "flex items-center gap-1.5",
			span { class: "h-2.5 w-2.5 rounded-full", style: "background:{color}" }
			"{label}"
		}
	}
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function rea_fit_scale(id) {
  const wrap = document.getElementById(id);
  const child = wrap && wrap.firstElementChild;
  if (!child || wrap.__reaRO) return;
  const fit = () => {
    const nw = child.offsetWidth, nh = child.offsetHeight;
    if (!nw || !nh) return;
    const s = Math.min(wrap.clientWidth / nw, wrap.clientHeight / nh);
    child.style.transform = 'scale(' + s + ')';
  };
  wrap.__reaRO = new ResizeObserver(fit);
  wrap.__reaRO.observe(wrap);
  fit();
}
"#)]
extern "C" {
	#[wasm_bindgen(js_name = rea_fit_scale)]
	fn fit_scale(id: &str);
}
