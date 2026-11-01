//! The dashboard reasons only about its own domain: it renders no brand header/footer and
//! no links off-site — whoever hosts it owns the surrounding nav. The one thing
//! left here is our own deployed-revision string, shown by `BuildTag`.

/// The deployed revision: the flake-passed `ANALYTICS_BUILD_REV` (hermetic builds have
/// no `.git`) falling back to build.rs's `git rev-parse` `GIT_HASH`.
pub fn build_rev() -> &'static str {
	option_env!("ANALYTICS_BUILD_REV").filter(|s| !s.is_empty()).unwrap_or(env!("GIT_HASH"))
}
