# analytics
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.92+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/analytics.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/analytics)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/analytics)
![Lines Of Code](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/analytics-loc.json)
<br>
[<img alt="ci errors" src="https://img.shields.io/github/actions/workflow/status/EV-Estate/analytics/errors.yml?branch=main&style=for-the-badge&style=flat-square&label=errors&labelColor=420d09" height="20">](https://github.com/EV-Estate/analytics/actions?query=branch%3Amain) <!--NB: Won't find it if repo is private-->
[<img alt="ci warnings" src="https://img.shields.io/github/actions/workflow/status/EV-Estate/analytics/warnings.yml?branch=main&style=for-the-badge&style=flat-square&label=warnings&labelColor=d16002" height="20">](https://github.com/EV-Estate/analytics/actions?query=branch%3Amain) <!--NB: Won't find it if repo is private-->

Portfolio analytics: the estate dashboard and the overview tile, as microfrontends
<!-- markdownlint-disable -->
<details>
<summary>
<h2>Installation</h2>
</summary>

## Deployment

Runs on the k3s/Flux2 cluster (Raspberry Pi 5, aarch64). analytics is on the v_flakes
**container standard**: `nix build .#container` produces the OCI image, the
release workflow pushes it to GHCR on a `vX.Y.Z` tag, and Flux rolls it out. The
cluster manifests are generated from this repo's container contract — see the
`gitops` repo.

### Release

Tag the repo; CI does the rest (no manual build/ship):

```sh
git tag v0.4.0 && git push --tags    # → release-container.yml builds on
                                      #   ubuntu-24.04-arm + pushes
                                      #   ghcr.io/ev-estate/analytics:v0.4.0
```

Manual build/push (when needed):

```sh
nix build .#container    # result → a docker-archive .tar.gz
skopeo copy docker-archive:result \
  docker://ghcr.io/ev-estate/analytics:v0.4.0
```

### Contract

Port `59079`, health `/health`, criticality `normal`. State lives on a PVC
mounted at `/data` (the image's `WorkingDir`; sqlite + properties + layout).

- **Config**: `deploy/config.nix`, baked into the image and passed as `--config`.
- **Secret env** (`ANALYTICS_ADMIN_TOKEN`, `GOOGLE_MAPS_KEY`): the `kubernetes-analytics`
  k8s Secret gitops applies (`envFrom`).
- **Routes**: the edge sends `https://estate.evinvest.ltd/mfe/analytics/*` here (see
  `docs/ARCHITECTURE.md` § Embedding).

</details>
<!-- markdownlint-restore -->

## Usage
TODO



<br>

<sup>
	This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a> and <a href="https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md">Tiger Style</a> (except "proper capitalization for acronyms": (VsrState, not VSRState) and formatting). For project's architecture, see <a href="./docs/ARCHITECTURE.md">ARCHITECTURE.md</a>.
</sup>

#### License

<sup>
	Licensed under <a href="LICENSE">Blue Oak 1.0.0</a>
</sup>

<br>

<sub>
	Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be licensed as above, without any additional terms or conditions.
</sub>

