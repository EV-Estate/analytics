#![feature(default_field_values)]

pub mod api;
pub mod app;
pub mod chrome;
pub mod dashboard;
pub mod i18n;
// The domain layer lives in a fullstack-free crate so the wasm embed bundle can
// share it; re-exported here so every `crate::domain::*` / `factors` / `error`
// path keeps resolving unchanged.
pub use analytics_core::{domain, error, factors};
pub mod map;
pub mod panels;
mod uikit;

// Server-only: `config` pulls v_utils' xdg/io (native-gated) + filesystem
// `LiveSettings`; `store` is the persistence layer. Neither is linked into the
// wasm client, which reaches the server purely through `api`.
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;
pub mod translations;

pub use app::App;
#[cfg(not(target_arch = "wasm32"))]
pub use ev_lib::architecture;
