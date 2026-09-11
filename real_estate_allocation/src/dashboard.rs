use std::rc::Rc;

use dioxus::prelude::*;
use dockviewers::dioxus::{Config, DockPanel, Group, GroupId, MinSize, PackedApi, PackedArea, PanelId, Saved, Step};

use crate::{
	api::load_default_layout,
	map::MapPanel,
	panels::{ChartPanel, DetailsPanel, LotsPanel, MediaPanel, PortfolioHeatmap, TopBar},
};

/// Every panel is at least a 2×2 tile; the dock scales the step with the viewport.
const MIN: MinSize = MinSize::Steps { w: Step(2), h: Step(2) };

#[component]
pub fn Dashboard() -> Element {
	let panels = use_signal(|| {
		vec![
			DockPanel {
				id: PanelId("map".into()),
				title: "Map".into(),
				content: rsx! { MapPanel {} },
			},
			DockPanel {
				id: PanelId("media".into()),
				title: "Media".into(),
				content: rsx! { MediaPanel {} },
			},
			DockPanel {
				id: PanelId("chart".into()),
				title: "Chart".into(),
				content: rsx! { ChartPanel {} },
			},
			DockPanel {
				id: PanelId("heatmap".into()),
				title: "Portfolio".into(),
				content: rsx! { PortfolioHeatmap {} },
			},
			DockPanel {
				id: PanelId("lots".into()),
				title: "Lots".into(),
				content: rsx! { LotsPanel {} },
			},
			DockPanel {
				id: PanelId("details".into()),
				title: "Details".into(),
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
	let config = Config {
		storage_key: Some("rea-dashboard".into()),
		on_save: Some(Rc::new(move |saved| match saved {
			Saved::Cached { band } => show_toast(toast, format!("Layout cached in this browser ({band})")),
			Saved::Published { band, json } => {
				spawn(async move {
					let msg = match crate::api::save_default_layout(json, band, crate::api::admin_token()).await {
						// An xl publish doubles as the `default` seed (see `save_default_layout`).
						Ok(()) => match band {
							dockviewers::core::Band::Xl => "Layout published (xl + default)".to_string(),
							band => format!("Layout published ({band})"),
						},
						Err(e) => {
							dioxus::logger::tracing::error!(%e, "publish default layout failed");
							"Publish failed".to_string()
						}
					};
					show_toast(toast, msg);
				});
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
		// off-screen overflow. Chromeless (see `Home`) — whatever the hosting shell
		// leaves us of it (--ev-shell-offset: 0 standalone, 4rem under the conductor).
		div { class: "flex h-[calc(100dvh-var(--ev-shell-offset,0px))] flex-col bg-background text-ink",
			TopBar {}
			div { class: "relative min-h-0 flex-1",
				PackedArea { panels, on_band: Some(on_band), config: Some(config) }
			}
			if let Some(msg) = toast() {
				div {
					class: "pointer-events-none fixed bottom-6 left-1/2 z-50 -translate-x-1/2 rounded
						border border-ink/20 bg-background/90 px-4 py-2 font-mono text-xs
						tracking-wider text-accent-debug shadow-lg",
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
