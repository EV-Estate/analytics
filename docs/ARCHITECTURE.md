# Architecture

A **page microfrontend** visualising the firm's property allocation: a docked
dashboard (portfolio map, price-series chart, media/documents, deal details) for a
selected property. A Dioxus 0.7 client (`<mfe-analytics-dashboard>`, CSR — no SSR, no
hydration) plus an axum server with the `#[server]` functions, on top of `ev`
(`architecture` + `uikit`) and `v_utils`.

It mounts inside a host page that hands it an OAuth token + admin list, so there is
**no login** — admin actions are gated on a token the host puts on
`window.__reaAdminToken`, re-checked server-side.

```mermaid
flowchart LR
  subgraph client [wasm client]
    app[App] --> dash[Dashboard]
    dash --> map[map::MapPanel]
    dash --> chart[panels::chart]
    dash --> media[panels::media]
    dash --> details[panels::details]
    map -. inline JS .-> gmaps[(Google Maps JS)]
  end
  subgraph server [axum server]
    api["api::#server fns"] --> store[store::SqliteStore]
    store --> db[(SQLite)]
    store --> disk[(file bytes on disk)]
  end
  client -- list/get/files/upload/bytes/am_i_admin/maps_key --> api
```

## Modules (`src/`)
- `domain` — `Property`, `PropertyState`, value objects (`Money`, `ResearchUrl`,
  …), `Id` tags, `Entity`/`AggregateRoot`, and the `InState` `Specification`.
- `error` — `DomainError` (thiserror).
- `store` *(server-only)* — `SqliteStore` + the leaf `PropertyRepository` port,
  schema, `Row` `TryFrom`s, and `seed`. Specs filter **in memory** (`spec.holds`);
  SQL pushdown is descoped. File bytes live on disk; only metadata is in SQLite.
- `api` — the **only** client↔server seam. `#[server]` fns pull the store/config
  out of the fullstack context via `consume_context`.
- `config` *(server-only)* — `AppConfig` (`maps_api_key`, `db_path`, `data_dir`,
  `admin_token`, `admins`) over `v_utils` `LiveSettings`.
- `app` / `dashboard` / `panels` — UI. `app::App` is the root (no router: the host
  owns the URL; deep links are read off its query, see **Embedding**). Selection is
  a root `Signal<Option<BuildingId>>` shared via context; each panel `use_resource`s
  keyed on it.
- `map` — **isolated** Google-Maps module; the only file touching the JS API. The
  inline-JS `extern` is fully `cfg(wasm32)`-gated, so the server build never links it.

## Property Info Standard
What we provide about a property (`domain::Property`). Required fields carry a
validating value object so a bad one can't enter the domain; optional fields are
`Option` and rendered only when present.

| Field | Type | Req. | Invariant / notes |
| --- | --- | --- | --- |
| `id` | `PropertyId` (`Id<PropertyTag>`, Uuid) | yes | minted on insert |
| `coords` | `Coords { lat, lng }` | yes | Google-Maps marker position |
| `price` | `Money` | yes | non-negative, finite |
| `state` | `PropertyState` | yes | `Purchased(Timestamp)` (carries the purchase instant) \| `Interesting` \| `Purchasing` |
| `research_url` | `ResearchUrl` | yes | non-empty http(s) — the reasoning post |
| `terms` | `Option<String>` | no | deal terms, free text |
| `deal` | `Option<DealStructure { equity_pct, debt_pct, notes }>` | no | deal structure |
| `loan` | `Option<LoanRates { rate_pct, term_years, lender }>` | no | rates if not bought outright |
| `additional_reasoning` | `Option<String>` | no | miscellaneous notes worth surfacing |
| `price_series` | `Vec<(Timestamp, f64)>` | — | dated weekly value estimates; **mock**, filled on read, never persisted |

Associated media (pics / pitch-deck / documents) are separate `PropertyFile`
records (`{ id, property_id, kind: Pic|PitchDeck|Document, filename, content_type }`),
not fields on `Property`.

## Embedding
Two microfrontends, both `ev_lib::mfe!` custom elements mounted in the host's light
DOM (never an iframe), both reading the locale off the host's `<html lang>`, both
served by this server under one prefix, `/mfe/analytics/`:

| element | crate | kind | mounted at |
|---|---|---|---|
| `<mfe-analytics-dashboard>` | `analytics_dashboard` | page | estate host `/analytics` (same origin) |
| `<mfe-analytics-overview>` | `analytics_embeds` (marketing tile) | component | landing / any host, cross-origin |

The edge routes `https://estate.evinvest.ltd/mfe/analytics/*` here. What the server
answers (`main.rs`):

| path | what | cache |
|---|---|---|
| `/mfe/analytics/{dashboard,overview}.js` | the stable entries hosts point at | `no-cache` + content `ETag` |
| `/mfe/analytics/{dashboard,overview}.css` | the stylesheets `mfe!` injects | `no-cache` + content `ETag` |
| `/mfe/analytics/<rev>/*` | wasm-bindgen glue, wasm, snippets (`<rev>` = content hash at boot) | `immutable` |
| `/mfe/analytics/seed/*` | the tile's banner images (`?v=` busted) | — |
| `/mfe/analytics/api/*` | the `#[server]` fns (root set by the bundle from its own origin) | — |
| `/mfe/analytics/api/embed/building/{id}` | the tile's plain JSON | — |
| `/health` | probe | — |
| `/api/embed/building/{id}` | legacy tile JSON for pre-0.4 bundles; to be removed | — |

Invariants:
- **Every URL a bundle emits derives from its own origin** (`bundle_origin()`, from
  `import.meta.url`). Hosts pass nothing; CORS is `*` so cross-origin hosts can load
  the module, the wasm and the JSON.
- **No `dioxus/fullstack` in the wasm graph.** It forces `dioxus-web/hydrate`, and a
  host page carries no hydration data; server-fn stubs come from `dioxus-fullstack`
  directly. Nothing in the client may use `asset!` (manganis URLs are `dx`'s).
- **The host owns the document**: fonts, tokens and preflight cascade in; our CSS is
  utilities-only (the tile's in the host's lowest layer, `reamfe`).
- **Deep links are the host URL's query** (`?building=`, `?appt=`, `?selection=`),
  read at mount and kept current with `replaceState` that preserves the host's state.
- **Shell-free tile.** The overview renders only its section; its deep links point at
  `<bundle origin>/analytics?building=…`.

## Persistence
- SQLite via `sqlx` (`runtime-tokio`, `sqlite`). One pool, schema run on startup.
- File bytes: `./data/properties/<property_id>/<file_id>__<filename>`.
- `price_series` is a **mock** (`v_utils::laplace_random_walk`, seeded per id),
  filled by `api::get_property` and never persisted. Points carry real dates anchored
  to the purchase instant, so a long-held property's series runs out before today and
  the chart projects a dotted tail to now.

## Build / run
Requires `nix develop` (the `wasm32` target, `wasm-bindgen-cli` pinned to the crate,
Tailwind v4).

```sh
nix run .#dev            # debug bundles → target/debug/public, then the server on :59079
                         # (bare harnesses: tile at /, dashboard at /analytics)
nix build .#mfe          # the release bundles; `.#default` = server + them in public/
```

Seeding runs on first launch when the DB is empty (~6 properties across all three
states, plus a sample pic). Config (incl. `maps_api_key`, `admin_token`) is read
through `LiveSettings`; without a Maps key the map shows a placeholder. The Maps key
is referrer-restricted: the host origin must be on its allowlist.
