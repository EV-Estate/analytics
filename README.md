# analytics
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.92+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/analytics.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/analytics)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/analytics)
![Lines Of Code](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/analytics-loc.json)
![Build time](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/analytics-build.json)
![Test time](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/analytics-test.json)
<br>
[![errors](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/analytics-errors.json)](https://github.com/EV-Estate/analytics/actions/workflows/errors.yml?query=branch%3Amain)
[![warnings](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/analytics-warnings.json)](https://github.com/EV-Estate/analytics/actions/workflows/warnings.yml?query=branch%3Amain)

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

<sup>Links for devs:</sup>
- <sup><a href="./docs/ARCHITECTURE.md">architecture</a></sup>
- <sup><a href="https://github.com/EV-Main/.github/blob/main/code/STYLE.md">style</a></sup>
- <sup><a href="https://github.com/EV-Main/.github/tree/main/code/conventions">conventions</a></sup>
- <sup><a href="https://github.com/EV-Main/.github/blob/main/code/conventions/git">contributing</a></sup>

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

