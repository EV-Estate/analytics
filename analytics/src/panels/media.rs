use dioxus::{html::HasFileData, prelude::*};
use ev_lib::{
	t,
	uikit::{Button, ButtonVariant, Card, CardContent, Skeleton, Tabs, TabsContent, TabsList, TabsTrigger},
};

use crate::{
	app::{SelectedAppt, SelectedBuilding},
	domain::{BuildingId, FileKind, PropertyFile},
	i18n::use_t,
};

#[component]
pub fn MediaPanel() -> Element {
	let tr = use_t();
	let selected = use_context::<SelectedBuilding>();
	let appt = use_context::<SelectedAppt>();
	let admin = use_resource(move || async move {
		let token = crate::api::admin_token();
		crate::api::am_i_admin(token).await.unwrap_or(false)
	});

	// Re-fetched after an upload by bumping `reload`.
	let mut reload = use_signal(|| 0u32);
	let files = use_resource(move || async move {
		reload();
		match selected() {
			Some(id) => crate::api::list_files(id, appt()).await.unwrap_or_default(),
			None => Vec::new(),
		}
	});

	let is_admin = *admin.read().as_ref().unwrap_or(&false);

	rsx! {
		Card { class: "flex h-full flex-col overflow-hidden",
			CardContent { class: "flex-1 overflow-y-auto",
				match selected() {
					None => rsx! { p { class: "text-muted-foreground text-sm", {t!(tr, "media.empty", "Select a building to view its media.")} } },
					Some(bid) => {
						let appt = appt();
						let files = files.read().clone().unwrap_or_default();
						rsx! {
							Tabs { default_value: "pics".to_string(),
								TabsList {
									TabsTrigger { value: "pics", {t!(tr, "media.pics", "Pics")} }
									TabsTrigger { value: "deck", {t!(tr, "media.deck", "Deck")} }
									TabsTrigger { value: "docs", {t!(tr, "media.docs", "Docs")} }
								}
								TabsContent { value: "pics", PicGrid { files: files.clone() } }
								TabsContent { value: "deck", FileList { files: files.clone(), kind: FileKind::PitchDeck } }
								TabsContent { value: "docs", FileList { files: files.clone(), kind: FileKind::Document } }
							}
							if is_admin {
								DropZone { building_id: bid, appt, on_uploaded: move |_| reload += 1 }
							}
						}
					}
				}
			}
		}
	}
}

#[component]
fn PicGrid(files: Vec<PropertyFile>) -> Element {
	let tr = use_t();
	let pics: Vec<_> = files.into_iter().filter(|f| f.kind == FileKind::Pic).collect();
	if pics.is_empty() {
		return rsx! { p { class: "text-muted-foreground text-sm", {t!(tr, "media.noPictures", "No pictures yet.")} } };
	}
	rsx! {
		div { class: "grid grid-cols-2 gap-2 pt-2",
			for f in pics {
				Pic { file: f }
			}
		}
	}
}

#[component]
fn Pic(file: PropertyFile) -> Element {
	let fid = file.id;
	let content_type = file.content_type.clone();
	let bytes = use_resource(move || async move { crate::api::file_bytes(fid).await.ok() });
	let src = bytes.read().as_ref().and_then(|o| o.as_ref()).map(|b| data_url(&content_type, b));

	rsx! {
		div { class: "aspect-video",
			match src {
				Some(src) => rsx! { img { class: "w-full h-full object-cover rounded-md", src } },
				None => rsx! { Skeleton { class: "w-full h-full" } },
			}
		}
	}
}

#[component]
fn FileList(files: Vec<PropertyFile>, kind: FileKind) -> Element {
	let tr = use_t();
	let items: Vec<_> = files.into_iter().filter(|f| f.kind == kind).collect();
	if items.is_empty() {
		return rsx! { p { class: "text-muted-foreground text-sm", {t!(tr, "media.nothingHere", "Nothing here yet.")} } };
	}
	rsx! {
		div { class: "flex flex-col gap-2 pt-2",
			for f in items {
				Download { file: f }
			}
		}
	}
}

#[component]
fn Download(file: PropertyFile) -> Element {
	let fid = file.id;
	let content_type = file.content_type.clone();
	let bytes = use_resource(move || async move { crate::api::file_bytes(fid).await.ok() });
	let href = bytes.read().as_ref().and_then(|o| o.as_ref()).map(|b| data_url(&content_type, b));

	rsx! {
		match href {
			Some(href) => rsx! {
				a { href, download: "{file.filename}", class: "w-full",
					Button { variant: ButtonVariant::Outline, class: "w-full justify-start", "{file.filename}" }
				}
			},
			None => rsx! { Skeleton { class: "h-9 w-full" } },
		}
	}
}

/// Admin-only upload surface: a `<input type=file>` plus a drop zone. Both read
/// bytes off the dioxus `FileData` and call the `upload_file` server fn (which
/// re-checks the admin token server-side).
#[component]
fn DropZone(building_id: BuildingId, appt: Option<u32>, on_uploaded: EventHandler<()>) -> Element {
	let tr = use_t();
	let do_upload = move |file: dioxus::html::FileData| {
		let on_uploaded = on_uploaded;
		async move {
			let filename = file.name();
			let content_type = file.content_type().unwrap_or_else(|| "application/octet-stream".to_string());
			let kind = kind_for(&content_type, &filename);
			if let Ok(bytes) = file.read_bytes().await {
				let token = crate::api::admin_token();
				if crate::api::upload_file(building_id, appt, kind, filename, content_type, bytes.to_vec(), token).await.is_ok() {
					on_uploaded.call(());
				}
			}
		}
	};

	rsx! {
		div {
			class: "mt-3 border border-dashed border-border rounded-md p-4 text-center text-sm text-muted-foreground",
			ondragover: move |e| e.prevent_default(),
			ondrop: move |e| {
				e.prevent_default();
				for file in e.files() {
					spawn(do_upload(file));
				}
			},
			{t!(tr, "media.dropFile", "Drop a file here, or")}
			input {
				r#type: "file",
				class: "block mx-auto mt-2 text-xs",
				onchange: move |e| {
					for file in e.files() {
						spawn(do_upload(file));
					}
				},
			}
		}
	}
}

fn kind_for(content_type: &str, filename: &str) -> FileKind {
	if content_type.starts_with("image/") {
		FileKind::Pic
	} else if filename.to_ascii_lowercase().contains("deck") || content_type.contains("presentation") {
		FileKind::PitchDeck
	} else {
		FileKind::Document
	}
}

fn data_url(content_type: &str, bytes: &[u8]) -> String {
	format!("data:{content_type};base64,{}", b64(bytes))
}

/// Minimal base64 (standard alphabet) so file bytes can ride in a `data:` URL
/// without pulling a crate outside the `ev_lib` + `v_utils` + renderer boundary.
fn b64(input: &[u8]) -> String {
	const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
	for chunk in input.chunks(3) {
		let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
		let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
		out.push(A[(n >> 18 & 63) as usize] as char);
		out.push(A[(n >> 12 & 63) as usize] as char);
		out.push(if chunk.len() > 1 { A[(n >> 6 & 63) as usize] as char } else { '=' });
		out.push(if chunk.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
	}
	out
}
