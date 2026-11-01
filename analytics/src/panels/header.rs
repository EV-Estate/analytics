use dioxus::prelude::*;
use ev_lib::{
	t,
	uikit::{Badge, BadgeVariant},
};

use crate::{
	app::{BuildingResource, SelectedAppt},
	domain::{ApartmentStatus, Building, PropertyStateKind},
	i18n::{status_label, use_t},
};

/// Persistent page header, two rows: breadcrumb (the only place the name appears),
/// then stats on the left and the one real action we have (open the research link)
/// on the right. At apartment level it also carries the back-to-building control.
/// The host micro-frontend owns the surrounding shell/nav, so there is intentionally
/// no sidebar here.
#[component]
pub fn TopBar() -> Element {
	let building = use_context::<BuildingResource>();
	let appt = use_context::<SelectedAppt>();

	rsx! {
		header { class: "sticky top-0 z-20 border-b border-border bg-background/90 backdrop-blur",
			// Inner column shares the body's max-width so the header aligns with the
			// content beneath it, while the border/background stay full-bleed.
			div { class: "mx-auto flex w-full max-w-[1200px] flex-col gap-2 py-3",
				match &*building.read() {
					Some(Some(b)) => rsx! { Loaded { building: b.clone(), appt: appt() } },
					Some(None) => rsx! { Empty {} },
					None => rsx! {
						div { class: "h-4 w-40 animate-pulse rounded bg-accent" }
						div { class: "h-9 w-64 animate-pulse rounded bg-accent" }
					},
				}
			}
		}
	}
}

#[component]
fn Loaded(building: Building, appt: Option<u32>) -> Element {
	let tr = use_t();
	let mut selected_appt = use_context::<SelectedAppt>();
	let apt = appt.and_then(|n| building.apartments.iter().find(|a| a.number == n).cloned());

	rsx! {
		Breadcrumb { building: building.name.clone(), appt }
		div { class: "flex flex-wrap items-center justify-between gap-3",
			div { class: "flex items-center gap-3",
				match &apt {
					Some(a) => rsx! {
						Badge { variant: status_variant(a.status), "{status_label(&tr, a.status)}" }
						match a.price {
							Some(p) => rsx! { span { class: "text-sm font-medium text-main-accent-t2", "{p}" } },
							None => rsx! { span { class: "text-sm font-medium text-warn", "?" } },
						}
					},
					None => rsx! {
						span { class: "text-sm font-medium text-main-accent-t1",
							"{building.lots_total()} lots"
						}
						match building.avg_price() {
							Some(p) => rsx! { span { class: "text-sm font-medium text-main-accent-t2", "avg {p}" } },
							None => rsx! { span { class: "text-sm font-medium text-warn", "?" } },
						}
					},
				}
			}
			div { class: "flex items-center gap-2",
				if apt.is_some() {
					button {
						r#type: "button",
						class: "inline-flex h-9 items-center justify-center gap-1.5 rounded-md border border-border bg-transparent px-4 text-sm font-medium transition-colors hover:bg-accent hover:text-accent-foreground",
						onclick: move |_| selected_appt.set(None),
						"← Back to building"
					}
				}
				a {
					href: "{building.research_url.as_str()}",
					target: "_blank",
					rel: "noopener noreferrer",
					class: "inline-flex h-9 items-center justify-center gap-1.5 rounded-md border border-border bg-transparent px-4 text-sm font-medium shadow-xs transition-colors hover:bg-accent hover:text-accent-foreground",
					{t!(tr, "header.openResearch", "Open research ↗")}
				}
			}
		}
	}
}

#[component]
fn Empty() -> Element {
	let tr = use_t();
	rsx! {
		nav { class: "flex items-center gap-2 text-sm text-muted-foreground",
			span { class: "text-foreground", {t!(tr, "header.buildings", "Buildings")} }
		}
		// Stats/actions are meaningless without a building, so row 2 becomes the mild
		// warning nudging towards selecting one. `h-9` matches the action row's height.
		div { class: "flex h-9 items-center text-sm font-medium text-main-accent-t3",
			{t!(tr, "header.empty", "Select a building on the map, to populate the dashboard")}
		}
	}
}

#[component]
fn Breadcrumb(building: String, appt: Option<u32>) -> Element {
	let tr = use_t();
	rsx! {
		nav { class: "flex items-center gap-2 text-sm text-muted-foreground",
			span { {t!(tr, "header.buildings", "Buildings")} }
			span { "›" }
			span { class: if appt.is_some() { "" } else { "text-foreground" }, "{building}" }
			if let Some(n) = appt {
				span { "›" }
				span { class: "text-foreground", {t!(tr, "header.apt", "Apt {n}", n = n)} }
			}
		}
	}
}

fn status_variant(status: ApartmentStatus) -> BadgeVariant {
	match status.portfolio_kind() {
		Some(PropertyStateKind::Purchased) => BadgeVariant::Success,
		Some(PropertyStateKind::Purchasing) => BadgeVariant::Secondary,
		Some(PropertyStateKind::Interesting) => BadgeVariant::Outline,
		None => BadgeVariant::Outline,
	}
}
