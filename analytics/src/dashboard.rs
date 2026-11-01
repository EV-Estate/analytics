use std::rc::Rc;

use dioxus::prelude::*;
use ev_lib::t;
use tilers::dioxus::{Config, DockPanel, Group, GroupId, MinSize, PackedApi, PackedArea, PanelId, Saved, Step};

use crate::{
	api::load_default_layout,
	i18n::use_t,
	map::MapPanel,
	panels::{ChartPanel, DetailsPanel, LotsPanel, MediaPanel, PortfolioHeatmap, TopBar},
};

/// Every panel is at least a 2×2 tile; the dock scales the step with the viewport.
const MIN: MinSize = MinSize::Steps { w: Step(2), h: Step(2) };

#[component]
pub fn Dashboard() -> Element {
	let tr = use_t();
	let panel_tr = tr.clone();
	let panels = use_signal(move || {
		vec![
			DockPanel {
				id: PanelId("map".into()),
				title: t!(panel_tr, "panel.map", "Map"),
				content: rsx! { MapPanel {} },
			},
			DockPanel {
				id: PanelId("media".into()),
				title: t!(panel_tr, "panel.media", "Media"),
				content: rsx! { MediaPanel {} },
			},
			DockPanel {
				id: PanelId("chart".into()),
				title: t!(panel_tr, "panel.chart", "Chart"),
				content: rsx! { ChartPanel {} },
			},
			DockPanel {
				id: PanelId("heatmap".into()),
				title: t!(panel_tr, "panel.portfolio", "Portfolio"),
				content: rsx! { PortfolioHeatmap {} },
			},
			DockPanel {
				id: PanelId("lots".into()),
				title: t!(panel_tr, "panel.lots", "Lots"),
				content: rsx! { LotsPanel {} },
			},
			DockPanel {
				id: PanelId("details".into()),
				title: t!(panel_tr, "panel.details", "Details"),
				content: rsx! { DetailsPanel {} },
			},
		]
	});

	// One invocation per band entry (first measure, then every crossing), after the dock has already
	// tried this browser's own cached arrangement. Only the fallbacks are ours: the server's saved
	// seed for that band, then the built-in one. The band lands in the shared `SeedGroup` so the
	// build tag can show which seed is live.
	let mut seed_group = use_context::<crate::app::SeedGroup>();
	let on_band = Callback::new(move |mut api: PackedApi| {
		let band = api.band();
		seed_group.set(Some(band));
		if api.restored() {
			return;
		}
		spawn(async move {
			let loaded = load_default_layout(band).await;
			// A resize crossed into another band while this was in flight; that crossing ran its own
			// load-or-seed and applying this one over it would fight the newer arrangement.
			if api.band() != band {
				return;
			}
			match loaded {
				Ok(Some(json)) =>
					if let Err(e) = api.load(&json) {
						// A corrupt saved layout must not blank the dashboard; log it and seed the default.
						dioxus::logger::tracing::error!(?e, "saved layout corrupt — using built-in seed");
						seed(&mut api);
					},
				Ok(None) => seed(&mut api),
				Err(e) => {
					// A fetch failure for the optional default likewise degrades to the built-in seed.
					dioxus::logger::tracing::error!(%e, "load_default_layout failed — using built-in seed");
					seed(&mut api);
				}
			}
		});
	});

	// `Alt+S` is the dock's own per-band localStorage cache — this browser only, no server. `Alt+Shift+S`
	// publishes the arrangement as *everyone's* seed, which the server accepts only from an admin. The
	// toast reports either, auto-clearing after a beat.
	let toast = use_signal(|| None::<String>);
	// The band name is interpolated, not concatenated: `{band}` is a placeholder
	// the policy checks for, so a translation that drops it is refused rather
	// than silently rendering a toast that never says which band was saved.
	let toast_tr = tr.clone();
	let config = Config {
		storage_key: Some("rea-dashboard".into()),
		on_save: Some(Rc::new(move |saved| {
			let tr = toast_tr.clone();
			match saved {
				Saved::Cached { band } => show_toast(toast, t!(tr, "dashboard.layoutCached", "Layout cached in this browser ({band})", band = band.to_string())),
				Saved::Published { band, json } => {
					spawn(async move {
						let msg = match crate::api::save_default_layout(json, band, crate::api::admin_token()).await {
							// An xl publish doubles as the `default` seed (see `save_default_layout`).
							Ok(()) => match band {
								tilers::core::Band::Xl => t!(tr, "dashboard.layoutPublishedDefault", "Layout published (xl + default)"),
								band => t!(tr, "dashboard.layoutPublished", "Layout published ({band})", band = band.to_string()),
							},
							Err(e) => {
								dioxus::logger::tracing::error!(%e, "publish default layout failed");
								t!(tr, "dashboard.publishFailed", "Publish failed")
							}
						};
						show_toast(toast, msg);
					});
				}
			}
		})),
		..Default::default()
	};

	// The packed grid's `+` ("add window as a tab") button asks the host to open a tab in `group`.
	// This dashboard has a fixed panel set with nothing to spawn, so the button is inert.
	// ponytail: wire to a panel picker if/when runtime windows exist.
	use_context_provider(|| Callback::new(|_group: GroupId| {}));

	rsx! {
		// The dock fills exactly one viewport so panels never get clipped by an
		// off-screen overflow; the host page around it is chromeless.
		div { class: "flex h-dvh flex-col bg-background text-foreground",
			TopBar {}
			div { class: "relative min-h-0 flex-1",
				PackedArea { panels, on_band: Some(on_band), config: Some(config) }
			}
			if let Some(msg) = toast() {
				div {
					class: "pointer-events-none fixed bottom-6 left-1/2 z-50 -translate-x-1/2 rounded
						border border-main-mist/20 bg-main-black/90 px-4 py-2 font-mono text-xs
						tracking-wider text-main-accent-t1 shadow-lg",
					"{msg}"
				}
			}
		}
	}
}

/// The built-in arrangement: map+media tabbed, the rest packed beside them.
fn seed(api: &mut PackedApi) {
	api.reset();
	let map_group = Group {
		id: api.mint_group_id(),
		tabs: vec![PanelId("map".into()), PanelId("media".into())],
		active: 0,
	};
	api.place(map_group, 4, 3, MIN);
	for panel in ["chart", "heatmap", "lots", "details"] {
		let group = Group::new(api.mint_group_id(), PanelId(panel.into()));
		api.place(group, 4, 3, MIN);
	}
}

fn show_toast(mut toast: Signal<Option<String>>, msg: String) {
	toast.set(Some(msg));
	#[cfg(target_arch = "wasm32")]
	spawn(async move {
		gloo_timers::future::TimeoutFuture::new(2500).await;
		toast.set(None);
	});
}
