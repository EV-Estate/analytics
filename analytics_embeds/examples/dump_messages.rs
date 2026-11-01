#![feature(default_field_values)]
//! Writes `messages/en/common.json` out of the `t!` sites in `view.rs`.
//!
//! The Rust half of `evinvest-i18n-extract`. It parses nothing: `t!` registers
//! each `(key, English)` pair with the linker, so every site that *compiles*
//! into this binary is in the catalogue whether or not it ever renders — which
//! is why `view` is pulled in by `#[path]` here and never rendered.
//!
//! `tests/catalogue.rs` is the paired check: it fails if the committed file no
//! longer matches, which is the only thing that makes this generated file
//! trustworthy as `resolve_catalogue`'s English source.
//!
//! ```sh
//! cargo run -p analytics_embeds --example dump_messages
//! ```

#[path = "../src/i18n.rs"]
mod i18n;
#[path = "../src/view.rs"]
mod view;

fn main() {
	let catalogue = ev_lib::i18n::catalogue().unwrap_or_else(|conflicts| {
		for conflict in &conflicts {
			eprintln!("  {conflict}");
		}
		panic!("{} key(s) registered twice with different English", conflicts.len());
	});
	let path = concat!(env!("CARGO_MANIFEST_DIR"), "/messages/en/common.json");
	std::fs::write(path, analytics_core::i18n::serialise(&catalogue)).unwrap_or_else(|e| panic!("{path}: {e}"));
	println!("en: {} keys", catalogue.len());
}
