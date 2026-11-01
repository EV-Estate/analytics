//! Throwaway (delete together with `store::import_legacy`): reconstruct the
//! files-on-disk era in a temp dir, then assert the startup import moves every
//! byte into blobs, adopts the layout, renames the sources to `*.imported`, and
//! hard-errors on a missing file or an already-vanished legacy dir.
use std::path::{Path, PathBuf};

use analytics::{
	domain::{FileId, FileKind, PropertyFile},
	store::{BuildingRepository, SqliteStore, import_legacy, seed},
};

/// Seed a fresh blob-era DB, then de-blob it: bytes out to the legacy disk
/// layout, rows back to the `x''` the 0002 migration leaves. Returns the
/// original bytes per file id for the round-trip assert.
async fn fabricate_legacy(dir: &Path) -> Vec<(FileId, Vec<u8>)> {
	let _ = std::fs::remove_dir_all(dir);
	let db = dir.join("app.db");
	let store = SqliteStore::open(&db).await.expect("open store");
	seed(&store).await.expect("seed");

	// One apt-nested file so the `…/apt/<n>/…` path branch is exercised too.
	let building = store.list(None).await.expect("list")[0].id;
	store
		.add_file(
			PropertyFile {
				id: FileId::new(),
				building_id: building,
				apt: Some(3),
				kind: FileKind::Document,
				filename: "apt_doc.bin".into(),
				content_type: "application/octet-stream".into(),
			},
			&vec![7u8; 2048],
		)
		.await
		.expect("add apt file");

	let mut originals = Vec::new();
	for b in store.list(None).await.expect("list") {
		for f in store.list_files(b.id).await.expect("list files") {
			let bytes = store.file_content(f.id).await.expect("blob");
			let mut d = dir.join("properties").join(b.id.raw().to_string());
			if let Some(n) = f.apt {
				d = d.join("apt").join(n.to_string());
			}
			std::fs::create_dir_all(&d).expect("mkdir");
			std::fs::write(d.join(format!("{}__{}", f.id.raw(), f.filename)), &bytes).expect("write legacy file");
			originals.push((f.id, bytes));
		}
	}

	let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display())).await.expect("raw pool");
	sqlx::query("UPDATE property_files SET content = x''").execute(&pool).await.expect("de-blob");
	pool.close().await;

	std::fs::write(dir.join("dashboard_layout.json"), r#"{"xl":{"grid":"legacy"}}"#).expect("write legacy layout");
	originals
}

#[tokio::main]
async fn main() {
	let base = std::env::temp_dir().join("rea_legacy_import");

	// Happy path: everything lands in blobs, sources renamed.
	let dir: PathBuf = base.join("ok");
	let originals = fabricate_legacy(&dir).await;
	let store = SqliteStore::open(&dir.join("app.db")).await.expect("reopen");
	import_legacy(&store, &dir.join("app.db")).await.expect("import");
	for (id, bytes) in &originals {
		assert_eq!(&store.file_content(*id).await.expect("blob after import"), bytes, "byte round-trip");
	}
	assert_eq!(
		store.load_layout("", "xl").await.expect("load layout").as_deref(),
		Some(r#"{"grid":"legacy"}"#),
		"legacy layout adopted"
	);
	assert!(dir.join("properties.imported").is_dir(), "data dir renamed");
	assert!(dir.join("dashboard_layout.json.imported").is_file(), "layout renamed");
	assert!(!dir.join("properties").exists() && !dir.join("dashboard_layout.json").exists());
	// Idempotent end state: a second boot is a no-op.
	import_legacy(&store, &dir.join("app.db")).await.expect("import is a no-op after rename");

	// A missing source file must abort the whole import, leaving sources in place.
	let dir = base.join("missing_file");
	let originals = fabricate_legacy(&dir).await;
	let victim = &originals[0].0;
	let mut found = false;
	for e in walk(&dir.join("properties")) {
		if e.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&victim.raw().to_string())) {
			std::fs::remove_file(&e).expect("delete victim");
			found = true;
		}
	}
	assert!(found, "victim file located");
	let store = SqliteStore::open(&dir.join("app.db")).await.expect("reopen");
	let err = import_legacy(&store, &dir.join("app.db")).await.expect_err("missing file must be fatal");
	assert!(err.to_string().contains(&victim.raw().to_string()), "error names the missing file: {err}");
	assert!(dir.join("properties").is_dir(), "sources untouched on failure");

	// Placeholder rows with the legacy dir gone = bytes lost; must be fatal.
	let dir = base.join("dir_gone");
	fabricate_legacy(&dir).await;
	std::fs::remove_dir_all(dir.join("properties")).expect("lose the dir");
	std::fs::remove_file(dir.join("dashboard_layout.json")).expect("lose the layout");
	let store = SqliteStore::open(&dir.join("app.db")).await.expect("reopen");
	import_legacy(&store, &dir.join("app.db")).await.expect_err("empty blobs with no source must be fatal");

	println!("OK");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
	let mut out = Vec::new();
	for e in std::fs::read_dir(dir).expect("read_dir") {
		let p = e.expect("dir entry").path();
		if p.is_dir() { out.extend(walk(&p)) } else { out.push(p) }
	}
	out
}
