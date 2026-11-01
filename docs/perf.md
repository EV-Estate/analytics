# All Things Impacting Perf

## Bundle Size

### MFE wasm (`analytics_embeds`)

Measured on `analytics_embeds_bg.wasm` (2.3 MB, `wasm-bindgen --target web --release`)
with `twiggy top`; shallow bytes summed per crate. Recorded for later size work — none is a bug.

Free wins, not yet applied:
- **Debug name section — 721 KB (31%).** Unstripped wasm-bindgen symbol names. Strip for prod
  with `wasm-opt -Oz` / `wasm-strip`; keep in dev (readable browser stack traces). Biggest lever.
- `.rodata` static data — 339 KB (14%). Shrinks somewhat under `wasm-opt -Oz`.

Heavy crates worth revisiting (code):
- **serde_json — 146 KB.** Deserializes `Building` over the wire. A leaner format (postcard/
  bincode) or hand-rolled deser cuts most of it. Invasive (type shared with the server).
- **jiff — 78 KB.** Datetime, via `domain` (price-series timestamps). Client only displays;
  could receive epoch/pre-formatted values and drop jiff client-side. Medium effort.
- **tailwind_fuse — 67 KB.** The `cn!` class-merge engine (uikit). Only removable by precomputing
  classes. Invasive, low priority.
- **url — 40 KB + idna — 29 KB.** URL parsing pulled by the server-fn/fetch client. `idna`
  (punycode for international domains) is pure overhead for known origins. Both should DCE out
  once the MFE drops `fullstack` and calls the server via raw `web-sys` fetch (string URLs). Do NOT
  reintroduce via `reqwest` — it re-pulls `url`.

Floor (not removable): dioxus core+web+signals ~420 KB.

Already DCE'd (not concerns): dashboard/map/panels/dockview/plotly ~7 KB total; miette/syntect/onig 0 KB.

