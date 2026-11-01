use dioxus::prelude::*;
use ev_lib::{
	t,
	uikit::{Card, CardContent, Skeleton},
};

use crate::{
	app::{BuildingResource, SelectedAppt},
	domain::{Apartment, Building},
	i18n::use_t,
};

#[component]
pub fn ChartPanel() -> Element {
	let tr = use_t();
	let building = use_context::<BuildingResource>();
	let appt = use_context::<SelectedAppt>();

	rsx! {
		Card { class: "flex h-full flex-col overflow-hidden",
			CardContent { class: "flex flex-1 flex-col gap-4",
				match &*building.read() {
					Some(Some(b)) => {
						let apt = appt().and_then(|n| b.apartments.iter().find(|a| a.number == n).cloned());
						rsx! { ChartBody { building: b.clone(), apt } }
					}
					Some(None) => rsx! { p { class: "text-sm text-muted-foreground", {t!(tr, "chart.empty", "Select a building on the map.")} } },
					None => rsx! { Skeleton { class: "h-full w-full" } },
				}
			}
		}
	}
}

#[component]
fn ChartBody(building: Building, apt: Option<Apartment>) -> Element {
	let tr = use_t();
	// Apartment view shows the lot's own price; building view the mean across lots.
	let (label, price) = match &apt {
		Some(a) => (t!(tr, "details.price", "Price"), a.price),
		None => (t!(tr, "details.avgApartmentPrice", "Avg apt. price"), building.avg_price()),
	};
	rsx! {
		div { class: "flex flex-col gap-0.5",
			span { class: "text-xs uppercase tracking-wide text-muted-foreground", "{label}" }
			match price {
				Some(p) => rsx! { span { class: "font-serif text-3xl font-semibold", "{p}" } },
				None => rsx! { span { class: "font-serif text-3xl font-semibold text-warn", "?" } },
			}
		}
		PriceChart { building, apt }
	}
}

/// Plotly price line. At apartment level the axis spans purchase→now: the owned
/// stretch is full colour, any pre-purchase estimates are dimmed, and a stale tail is
/// carried to today as a dotted projection. At building level it plots the per-week
/// mean across every lot's series (no single purchase instant, so full colour).
#[component]
fn PriceChart(building: Building, apt: Option<Apartment>) -> Element {
	#[cfg(target_arch = "wasm32")]
	{
		use_effect(move || {
			let (points, purchase_ms): (Vec<(i64, f64)>, f64) = match &apt {
				Some(a) => (
					a.price_series.iter().map(|(t, v)| (t.as_millisecond(), *v)).collect(),
					match a.status {
						crate::domain::ApartmentStatus::Purchased(ts) => ts.as_millisecond() as f64,
						_ => f64::NAN,
					},
				),
				None => (building_mean_series(&building), f64::NAN),
			};
			let (Some(first), Some(last)) = (points.first().map(|p| p.1), points.last().map(|p| p.1)) else {
				return;
			};
			let color_key = if last >= first { "up" } else { "down" };
			plot_prices("rea-chart", &serde_json::to_string(&points).expect("Vec<(i64,f64)> serializes"), purchase_ms, color_key);
		});
	}
	#[cfg(not(target_arch = "wasm32"))]
	let _ = (building, apt);
	rsx! {
		div { id: "rea-chart", class: "min-h-0 w-full flex-1" }
	}
}

/// Per-week mean of every lot's value series (timestamps bucketed to the week), so the
/// building chart is a single line over the same axis as the per-lot one.
#[cfg(target_arch = "wasm32")]
fn building_mean_series(building: &Building) -> Vec<(i64, f64)> {
	use std::collections::BTreeMap;
	const WEEK_MS: i64 = 7 * 24 * 3600 * 1000;
	let mut buckets: BTreeMap<i64, (f64, u32)> = BTreeMap::new();
	for a in &building.apartments {
		for (t, v) in &a.price_series {
			let e = buckets.entry(t.as_millisecond() / WEEK_MS).or_default();
			e.0 += *v;
			e.1 += 1;
		}
	}
	buckets.into_iter().map(|(k, (sum, n))| (k * WEEK_MS, sum / n as f64)).collect()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
function __reaLine(x, y, color) {
  return { x: x, y: y, type: 'scatter', mode: 'lines+markers',
    line: { color: color, width: 2, shape: 'spline' },
    marker: { color: color, size: 6 },
    hovertemplate: '$%{y:.1f}<extra></extra>' };
}

export function rea_plot_prices(elId, pointsJson, purchaseMs, colorKey) {
  if (!window.Plotly) { setTimeout(() => rea_plot_prices(elId, pointsJson, purchaseMs, colorKey), 200); return; }
  const el = document.getElementById(elId);
  if (!el) return;
  const pts = JSON.parse(pointsJson);
  if (!pts.length) { window.Plotly.purge(el); return; }

  const now = Date.now();
  const P = isNaN(purchaseMs) ? null : purchaseMs;
  const pal = colorKey === 'down'
    ? { full: 'oklch(0.62 0.19 25)',  dim: 'oklch(0.52 0.05 25)' }
    : { full: 'oklch(0.70 0.14 152)', dim: 'oklch(0.58 0.04 152)' };

  const ms = pts.map(p => p[0]);
  const ys = pts.map(p => p[1]);
  const lastMs = ms[ms.length - 1];
  const lastY = ys[ys.length - 1];
  const D = (arr) => arr.map(m => new Date(m));

  // Linearly interpolate the value at the purchase instant so the dim→full seam sits
  // exactly on the purchase date rather than snapping to the nearest weekly point.
  const interp = (t) => {
    if (t <= ms[0]) return ys[0];
    if (t >= lastMs) return lastY;
    for (let i = 1; i < ms.length; i++) {
      if (ms[i] >= t) { const f = (t - ms[i - 1]) / (ms[i] - ms[i - 1]); return ys[i - 1] + f * (ys[i] - ys[i - 1]); }
    }
    return lastY;
  };

  const traces = [];
  if (P !== null && P > ms[0] && P < lastMs) {
    const vp = interp(P);
    const preMs = [], preY = [], postMs = [], postY = [];
    for (let i = 0; i < ms.length; i++) {
      if (ms[i] <= P) { preMs.push(ms[i]); preY.push(ys[i]); }
      else { postMs.push(ms[i]); postY.push(ys[i]); }
    }
    preMs.push(P); preY.push(vp);
    postMs.unshift(P); postY.unshift(vp);
    traces.push(__reaLine(D(preMs), preY, pal.dim));
    traces.push(__reaLine(D(postMs), postY, pal.full));
  } else if (P !== null && P >= lastMs) {
    traces.push(__reaLine(D(ms), ys, pal.dim));
  } else {
    traces.push(__reaLine(D(ms), ys, pal.full));
  }

  // Stale tail: carry the last estimate to today as a dotted projection.
  if (lastMs < now) {
    traces.push({ x: D([lastMs, now]), y: [lastY, lastY], type: 'scatter', mode: 'lines',
      line: { color: pal.full, width: 2, dash: 'dot' }, hoverinfo: 'skip', showlegend: false });
  }

  const left = (P !== null) ? Math.min(P, ms[0]) : ms[0];
  const layout = {
    margin: { l: 56, r: 14, t: 8, b: 28 },
    paper_bgcolor: 'rgba(0,0,0,0)', plot_bgcolor: 'rgba(0,0,0,0)',
    font: { color: '#9a9486', size: 11 },
    xaxis: { type: 'date', showgrid: false, range: [new Date(left), new Date(now)] },
    yaxis: { tickprefix: '$', showgrid: true, gridcolor: 'rgba(230,225,211,0.08)', zeroline: false },
    showlegend: false,
  };
  window.Plotly.react(el, traces, layout, { displayModeBar: false, responsive: true });

  // responsive:true only fires on window resize; observe the element so the chart
  // also reflows when its container resizes (split-pane drag, sibling collapse).
  if (!el.__reaRO) {
    el.__reaRO = new ResizeObserver(() => { if (window.Plotly) window.Plotly.Plots.resize(el); });
    el.__reaRO.observe(el);
  }
}
"#)]
extern "C" {
	#[wasm_bindgen(js_name = rea_plot_prices)]
	fn plot_prices(el_id: &str, points_json: &str, purchase_ms: f64, color_key: &str);
}
