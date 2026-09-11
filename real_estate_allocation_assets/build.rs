//! Writes the two Tailwind inputs the kit owns into `real_estate_allocation/`,
//! replacing the hand-vendored copies that carried "keep in sync manually"
//! headers.
//!
//! Lives in its own crate rather than in `real_estate_allocation/build.rs`,
//! which v_flakes generates and overwrites on every `nix develop`.
//!
//! Both outputs stay committed. Tailwind runs from the flake's `postPatch` —
//! before cargo, so before this script — and the pure source excludes
//! gitignored files, so a generated-but-untracked input would simply be absent
//! there. Committed, `git diff` after a bump is the review.

fn main() {
	let site = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.expect("crate is one level under the repo root")
		.join("real_estate_allocation");

	// Tailwind can neither scan nor `@import` a crates.io checkout: `ev_lib` is
	// unpacked at no path a committed `@source` could reach. The crate ships
	// both as data instead.
	write(&site.join("uikit_classes.txt"), ev_lib_classes::CLASS_INVENTORY);
	write(&site.join("assets/tokens.css"), ev_lib_classes::TOKENS_CSS);
}

/// Only touch the file when the bytes change, so a no-op build does not restat
/// every downstream `rerun-if-changed`.
fn write(path: &std::path::Path, body: &str) {
	if std::fs::read_to_string(path).is_ok_and(|on_disk| on_disk == body) {
		return;
	}
	std::fs::write(path, body).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}
