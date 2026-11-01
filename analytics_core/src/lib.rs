pub mod domain;
pub mod error;
pub mod factors;
pub mod i18n;

/// Standard placeholder for a value that isn't available / not yet known (live-fetched
/// data absent at snapshot time, a figure still loading). Shared by the live embed and
/// its static snapshot so both read identically. `"ERR"` stays reserved for a detected
/// data fault — a value that *should* exist but is missing.
pub const MISSING: &str = "—";
