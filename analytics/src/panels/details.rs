use dioxus::prelude::*;
use ev_lib::{
	t,
	uikit::{Card, CardContent, Skeleton, Tooltip, TooltipContent, TooltipTrigger},
};

use crate::{
	app::{BuildingResource, SelectedAppt},
	domain::{Apartment, Building, ConstructionStatus},
	i18n::{status_label, use_t},
};

#[component]
pub fn DetailsPanel() -> Element {
	let tr = use_t();
	let building = use_context::<BuildingResource>();
	let appt = use_context::<SelectedAppt>();

	rsx! {
		Card { class: "flex h-full flex-col",
			CardContent { class: "flex-1 overflow-y-auto",
				match &*building.read() {
					Some(Some(b)) => {
						match appt().and_then(|n| b.apartments.iter().find(|a| a.number == n).cloned()) {
							Some(a) => rsx! { ApartmentDetails { apt: a } },
							None => rsx! { BuildingDetails { building: b.clone() } },
						}
					}
					Some(None) => rsx! { p { class: "text-sm text-muted-foreground", {t!(tr, "details.empty", "Select a building to see its terms.")} } },
					None => rsx! { Skeleton { class: "h-48 w-full" } },
				}
			}
		}
	}
}

#[component]
fn ApartmentDetails(apt: Apartment) -> Element {
	let tr = use_t();
	let status = status_label(&tr, apt.status);
	rsx! {
		div { class: "flex flex-col",
			Kv { label: t!(tr, "details.apartment", "Apartment"), "#{apt.number}" }
			Kv { label: t!(tr, "details.status", "Status"), value_class: "text-main-accent-t3", "{status}" }
			match apt.price {
				Some(p) => rsx! { Kv { label: t!(tr, "details.price", "Price"), value_class: "text-main-accent-t3", "{p}" } },
				None => rsx! { Kv { label: t!(tr, "details.price", "Price"), value_class: "text-warn", "?" } },
			}
		}
	}
}

#[component]
fn BuildingDetails(building: Building) -> Element {
	let tr = use_t();
	rsx! {
		div { class: "flex flex-col",
			match building.avg_price() {
				Some(p) => rsx! { Kv { label: t!(tr, "details.avgApartmentPrice", "Avg apt. price"), value_class: "text-main-accent-t3", "{p}" } },
				None => rsx! { Kv { label: t!(tr, "details.avgApartmentPrice", "Avg apt. price"), value_class: "text-warn", "?" } },
			}

			Kv { label: t!(tr, "details.lots", "Lots"), "{building.lots_total()}" }

			if let Some(dev) = building.developer.as_ref() {
				DeveloperKv { name: dev.clone() }
			}

			Kv {
				label: t!(tr, "details.construction", "Construction"),
				value_class: match building.construction {
					ConstructionStatus::Completed => "text-main-accent-t2",
					ConstructionStatus::UnderConstruction => "text-warn",
				},
				"{building.construction}"
			}

			if building.target_appreciation > 0.0 {
				Kv { label: t!(tr, "details.targetAppreciation", "Target appreciation"), value_class: "text-main-accent-t3", "{building.target_appreciation:.0}% / yr" }
			}

			if let Some(deal) = building.deal.as_ref() {
				Kv { label: t!(tr, "details.equityDebt", "Equity / Debt"), "{deal.equity_pct:.0}% / {deal.debt_pct:.0}%" }
			}

			if let Some(loan) = building.loan.as_ref() {
				Kv { label: t!(tr, "details.loanRate", "Loan rate"), value_class: "text-main-accent-t1", "{loan.rate_pct:.2}%" }
				Kv { label: t!(tr, "details.term", "Term"), "{loan.term_years} yr" }
				Kv { label: t!(tr, "details.lender", "Lender"), "{loan.lender}" }
			}

			if let Some(terms) = crate::translations::terms(&building, tr.locale()) {
				Note { label: t!(tr, "details.terms", "Terms"), "{terms}" }
			}

			if let Some(notes) = building.deal.as_ref().and_then(|d| d.notes.as_ref()) {
				Note { label: t!(tr, "details.dealNotes", "Deal notes"), "{notes}" }
			}

			if let Some(reasoning) = crate::translations::reasoning(&building, tr.locale()) {
				Note { label: t!(tr, "details.reasoning", "Reasoning"), "{reasoning}" }
			}
		}
	}
}

/// A label/value row. Hairline-separated from the previous row via a top border
/// (first row drops it), matching the kit's deal-terms rhythm.
#[component]
fn Kv(label: String, #[props(default)] value_class: String, children: Element) -> Element {
	rsx! {
		div { class: "flex items-center justify-between gap-4 border-t border-border py-2.5 first:border-t-0",
			span { class: "text-sm text-muted-foreground", "{label}" }
			span { class: "text-right text-sm font-semibold {value_class}", {children} }
		}
	}
}

/// One of the three headline stats (yield / appreciation / status): an eyebrow label
/// over a serif value, laid out as a column in the top grid.
#[component]
fn Stat(label: String, #[props(default)] value_class: String, children: Element) -> Element {
	rsx! {
		div { class: "flex flex-col gap-1",
			span { class: "text-[10px] font-medium uppercase tracking-wide text-muted-foreground", "{label}" }
			span { class: "font-serif text-lg font-semibold {value_class}", {children} }
		}
	}
}

/// Developer row. Resolves the developer's note server-side and, when present,
/// surfaces it on hover over the name.
#[component]
fn DeveloperKv(name: String) -> Element {
	let tr = use_t();
	let lookup = name.clone();
	let dev = use_resource(move || {
		let lookup = lookup.clone();
		async move { crate::api::get_developer(lookup).await.ok().flatten() }
	});
	let note = dev.read().as_ref().and_then(|o| o.as_ref()).map(|d| d.note.clone()).filter(|n| !n.trim().is_empty());

	rsx! {
		match note {
			Some(note) => rsx! {
				Kv { label: t!(tr, "details.developer", "Developer"),
					Tooltip {
						TooltipTrigger { class: "cursor-help text-sm font-semibold underline decoration-dotted underline-offset-4", "{name}" }
						TooltipContent { class: "max-w-xs text-left text-xs font-normal normal-case", "{note}" }
					}
				}
			},
			None => rsx! { Kv { label: t!(tr, "details.developer", "Developer"), "{name}" } },
		}
	}
}

/// A free-text block (terms, notes, reasoning) under an eyebrow label.
#[component]
fn Note(label: String, children: Element) -> Element {
	rsx! {
		div { class: "mt-2 flex flex-col gap-1.5 border-t border-border pt-3",
			span { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "{label}" }
			p { class: "text-sm leading-relaxed text-muted-foreground", {children} }
		}
	}
}
