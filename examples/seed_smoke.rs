//! Throwaway: seed a temp DB and assert each building round-trips from its stored
//! document with its lots + bundled pics intact.
use analytics::store::{BuildingRepository, SqliteStore, seed};

#[tokio::main]
async fn main() {
	let dir = std::env::temp_dir().join("rea_seed_smoke");
	let _ = std::fs::remove_dir_all(&dir);
	let store = SqliteStore::open(&dir.join("app.db")).await.expect("open store");
	seed(&store).await.expect("seed");

	let all = store.list(None).await.expect("list");
	assert_eq!(all.len(), 7, "expected 7 portfolio buildings");
	for b in &all {
		// The rigidity we just bought: a building never persists without its lots.
		assert!(!b.apartments.is_empty(), "{} persisted with no lots", b.name);
		let files = store.list_files(b.id).await.expect("list files");
		assert!(!files.is_empty(), "{} has no pics", b.name);
		for f in &files {
			let bytes = store.file_content(f.id).await.expect("pic blob in db");
			assert!(bytes.len() > 1000, "{} pic suspiciously small", f.filename);
		}
		let dev = b.developer.as_ref().expect("seed sets a developer on every building");
		assert!(store.get_developer(dev).await.expect("get developer").is_some(), "{} references unknown developer {dev}", b.name);
		let price = b.avg_price().map(|m| m.to_string()).unwrap_or_else(|| "?".into());
		println!(
			"{:<42} {price:>9}  {:<18} {} lots  {} pics  dev={dev}",
			b.name,
			b.construction.as_ref(),
			b.apartments.len(),
			files.len()
		);
	}
	println!("OK");
}
