{
  inputs = {
    v_flakes.url = "github:valeratrades/v_flakes?ref=v1.6";
  };
  outputs = { self, v_flakes }:
    let
      inherit (v_flakes) flake-utils pre-commit-hooks;
    in
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import v_flakes.default_nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };
        # Canonical toolchain pinned in v_flakes — byte-identical across repos, so
        # the nix store dedups it and sccache cross-references compilations.
        rust = v_flakes.rs.default_nightly system;
        # `buildRustPackage` derivations (bin/mfe) use this one —
        # the dev toolchain isn't meant for hermetic package builds.
        build_rust = v_flakes.rs.build_nightly system;
        # rust-lld (the wasm32 linker) embeds a bad rpath on macOS — it looks for
        # libLLVM.dylib in bin/../lib but Nix puts it in <rust>/lib, so a wasm build
        # aborts (SIGABRT) at link time. Point the dynamic loader at <rust>/lib.
        # Darwin-only: the var doesn't exist on Linux's loader.
        dyldFallback = pkgs.lib.optionalString pkgs.stdenv.isDarwin
          ''export DYLD_FALLBACK_LIBRARY_PATH="${rust}/lib''${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"'';
        # v_flakes ships the treefmt hook; we extend it with the same `.#test`
        # derivation (cargo test / insta snapshots). Kept on pre-push, not
        # pre-commit — a full test run per commit is too slow; flip `stages` to
        # ["pre-commit"] if you want it on every commit.
        preCommitBase = v_flakes.files.preCommit { inherit pkgs; };
        pre-commit-check = pre-commit-hooks.lib.${system}.run (preCommitBase // {
          hooks = preCommitBase.hooks // {
            test = {
              enable = true;
              name = "nix run .#test (cargo test / insta snapshots)";
              entry = "${runTest}/bin/run-test";
              pass_filenames = false;
              stages = [ "pre-push" ];
            };
          };
        });
        pname = (pkgs.lib.importTOML ./analytics/Cargo.toml).package.name;
        version = (pkgs.lib.importTOML ./Cargo.toml).workspace.package.version;
        # mold is Linux-only (the adapter *throws* on Darwin at eval time).
        stdenv = if pkgs.stdenv.isDarwin then pkgs.stdenv else pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;

        # dev/prod serving port (deploy/config.nix binds the same in prod).
        port = "59079";

        # Pinned to match the workspace's `wasm-bindgen` (`=0.2.125`) — nixpkgs
        # ships a different minor, and a CLI/crate schema skew is a hard error at
        # `wasm-bindgen` time. Shadows `pkgs.wasm-bindgen-cli` (a `let` binding
        # wins over `with pkgs;`) wherever it's referenced below.
        wasm-bindgen-cli =
          let
            src = pkgs.fetchCrate {
              pname = "wasm-bindgen-cli";
              version = "0.2.125";
              hash = "sha256-zRawtjxMOdTMX+mZaiNR3YYfTiZJhf9qj7kXSSeMxrc=";
            };
          in
          pkgs.buildWasmBindgenCli {
            inherit src;
            cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
              inherit src;
              inherit (src) pname version;
              hash = "sha256-aZCfgR23Qb0Pn4Mm4ToMtuuRQqSJjXCR9li/VvP5CTM=";
            };
          };

        rs = v_flakes.rs {
          inherit pkgs rust;
          build = {
            deny = false;
            workspace = let deprecate_by = "v1.0.0"; in {
              "./analytics/" = [ "git_version" "log_directives" { deprecate = { by_version = deprecate_by; force = true; }; } ];
            };
          };
        };
        github = v_flakes.github {
          inherit pkgs pname rs;
          enable = true;
          lastSupportedVersion = "nightly-2026-06-16";
          containerRelease = { registry = "ghcr.io/ev-estate"; };
          jobs.default = true;
          lfs = false;
          gitignore.extra = ''
            **/node_modules/
            /data/
          '';
        };
        readme = v_flakes.readme-fw {
          inherit pkgs pname;
          defaults = true;
          lastSupportedVersion = "nightly-1.92";
          rootDir = ./.;
          badges = [ "msrv" "crates_io" "docs_rs" "loc" "ci" ];
        };
        combined = v_flakes.utils.combine { inherit rust; modules = [ rs github readme ]; };

        # ── MFE bundle steps (shared by `mfe` and `nix run .#dev`) ──────────
        # wasm-bindgen + utilities-only CSS + the tile's seed images into `$out`; the
        # server (main.rs `Bundle`) serves it under `/mfe/analytics/` and hashes it
        # into the immutable path itself.
        mfeCrates = "-p analytics_dashboard -p analytics_embeds";
        mfeSteps = { profile, out }: ''
          for name in analytics_dashboard analytics_embeds; do
            wasm-bindgen --target web --no-typescript --out-dir "${out}" --out-name "$name" \
              "target/wasm32-unknown-unknown/${profile}/$name.wasm"
          done
          ( cd analytics && tailwindcss -i ./dashboard.css -o "${out}/dashboard.css" && tailwindcss -i ./overview.css -o "${out}/overview.css" )
          cp -r analytics/assets/seed "${out}/seed"
        '';

        # ── dev orchestrator ─────────────────────────────────────────────────
        # `nix run .#dev` → build both MFE bundles (debug) into `public/` beside the
        # debug server binary, then run the server. Bare dev harnesses: the tile at
        # http://127.0.0.1:${port}/, the dashboard at /analytics (the estate host's
        # `/analytics` is the real one). No hot reload: rerun after changes.
        #
        # IMPORTANT: resolve the repo at *runtime* via `git rev-parse`, never
        # `toString ./.` — the latter locks the wrapper to the read-only
        # /nix/store snapshot, where cargo (target/) can't write. The cargo steps
        # delegate to `nix develop` for the `.cargo` accelerators a bare PATH lacks.
        runDev = pkgs.writeShellApplication {
          name = "run-dev";
          runtimeInputs = [ pkgs.git wasm-bindgen-cli pkgs.tailwindcss_4 ];
          text = ''
            repo="$(git rev-parse --show-toplevel)"
            cd "$repo"
            public="$repo/target/debug/public"
            rm -rf "$public"; mkdir -p "$public"
            nix develop "$repo" --command env RUSTFLAGS='--cfg=web_sys_unstable_apis --cfg=getrandom_backend="wasm_js"' \
              cargo build ${mfeCrates} --target wasm32-unknown-unknown
            ${mfeSteps { profile = "debug"; out = "$public"; }}
            exec nix develop "$repo" --command cargo run -p analytics
          '';
        };

        # ── dev refresh from prod ───────────────────────────────────────────
        # `nix run .#pull-prod-db` → replace the local dev DB with prod's,
        # restored from the litestream R2 replica — the same replica that backs
        # prod up and bootstraps fresh PVCs, so a successful pull doubles as the
        # backup-restore drill. Prod is authoritative; dev only ever pulls
        # (per-user layouts make a dev push actively destructive). Creds:
        # R2_ACCESS_KEY_ID / R2_SECRET_ACCESS_KEY in the environment.
        runPullProdDb =
          let
            # Mirrors the prod sidecar's replica block (gitops owns that copy);
            # the `/data/app.db` key is just the replica's index name here.
            litestreamCfg = pkgs.writeText "litestream-pull.yml" ''
              dbs:
                - path: /data/app.db
                  replicas:
                    - type: s3
                      endpoint: https://1dbedc392b294bdef442b64e9030ba96.r2.cloudflarestorage.com
                      bucket: ev-invest-state
                      path: litestream/ev-estate/analytics-data/app.db
            '';
          in
          pkgs.writeShellApplication {
            name = "pull-prod-db";
            runtimeInputs = with pkgs; [ litestream coreutils ];
            text = ''
              : "''${R2_ACCESS_KEY_ID:?R2_ACCESS_KEY_ID not set}"
              : "''${R2_SECRET_ACCESS_KEY:?R2_SECRET_ACCESS_KEY not set}"
              export LITESTREAM_ACCESS_KEY_ID="$R2_ACCESS_KEY_ID"
              export LITESTREAM_SECRET_ACCESS_KEY="$R2_SECRET_ACCESS_KEY"
              out="''${XDG_DATA_HOME:-$HOME/.local/share}/analytics/app.db"
              mkdir -p "$(dirname "$out")"
              rm -f "$out" "$out-wal" "$out-shm"
              litestream restore -config ${litestreamCfg} -o "$out" /data/app.db
              echo "restored prod DB → $out"
            '';
          };

        # ── test suite: cargo test (runs the insta snapshots) ──────────────
        # Delegates to `nix develop` so cargo gets the devShell toolchain (the
        # `.cargo` sccache/cranelift/mold accelerators a bare app PATH lacks),
        # exactly like runDev does.
        runTest = pkgs.writeShellApplication {
          name = "run-test";
          runtimeInputs = with pkgs; [ git ];
          text = ''
            repo="$(git rev-parse --show-toplevel)"
            echo "▶ cargo test (insta snapshots)"
            exec nix develop "$repo" --command cargo test
          '';
        };

      in
      let
        rustc = build_rust;
        cargo = build_rust;
        rustPlatform = pkgs.makeRustPlatform {
          inherit rustc cargo stdenv;
        };
        # The hermetic sandbox has no `.git`, so build.rs's `GIT_HASH` is "unknown" there;
        # `chrome::build_rev` reads this instead (server and wasm both render the label).
        ANALYTICS_BUILD_REV = self.shortRev or self.dirtyShortRev or "";
        # `.cargo` holds dev-only accelerators (sccache rustc-wrapper, cranelift,
        # mold) the hermetic sandbox lacks — drop it so the pure build uses nix's
        # own toolchain instead of failing on a missing `sccache` on PATH.
        pureSrc = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: _type: baseNameOf path != ".cargo";
        };
        bin = rustPlatform.buildRustPackage {
          inherit pname version;

          buildInputs = with pkgs; [
            openssl.dev
            sqlite
          ];
          nativeBuildInputs = with pkgs; [ pkg-config cmake perl pkgs.rustPlatform.bindgenHook removeReferencesTo ] ++ pkgs.lib.optionals (!pkgs.stdenv.isDarwin) [ pkgs.mold ];

          cargoLock.lockFile = ./Cargo.lock;
          src = pureSrc;

          inherit ANALYTICS_BUILD_REV;

          # The server reads the MFE bundles from `public/` next to its own binary
          # (resolved through the realpath, so it must share the store path).
          # Panic-location strings bake the toolchain's std source path into the
          # binary and wasm — pure data, but nix's reference scanner would drag the
          # whole toolchain into the image. Scrub just those paths.
          postInstall = ''
            cp -r --no-preserve=mode ${mfe} $out/bin/public
            remove-references-to -t ${rust} -t ${build_rust} $out/bin/analytics
            find $out/bin/public -name '*.wasm' -exec remove-references-to -t ${rust} -t ${build_rust} {} +
          '';
          doCheck = false;
        };

        # ── both MFE bundles, served by the server itself (copied into bin's `public/`) ──
        # All producer boilerplate (custom-element registration, start fn) is the
        # `ev_lib::mfe!` macro; these steps are the manganis-free packaging `dx` can't
        # do. `wasm-bindgen-cli` is pinned to the `wasm-bindgen` crate (=0.2.125).
        # web_sys_unstable_apis: dioxus-web touches unstable web-sys. getrandom_backend:
        # getrandom 0.3 (transitive via ahash) selects its wasm backend by cfg.
        mfe = rustPlatform.buildRustPackage {
          pname = "${pname}-mfe";
          inherit version ANALYTICS_BUILD_REV;
          src = pureSrc;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ wasm-bindgen-cli pkgs.tailwindcss_4 pkgs.binaryen ];
          buildPhase = ''
            runHook preBuild
            ${dyldFallback}
            export RUSTFLAGS='--cfg=web_sys_unstable_apis --cfg=getrandom_backend="wasm_js"'
            cargo build ${mfeCrates} --target wasm32-unknown-unknown --release --offline
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            mkdir -p "$out"
            ${mfeSteps { profile = "release"; out = "$out"; }}
            for wasm in "$out"/*_bg.wasm; do
              wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-reference-types --enable-mutable-globals --enable-multivalue \
                "$wasm" -o "$wasm"
            done
            runHook postInstall
          '';
          doCheck = false;
        };

        # bin's closure is pulled via the Entrypoint store-path ref.
        # Secret-free prod config baked into the image; its `.env` fields pull the
        # two secrets (GOOGLE_MAPS_KEY, ANALYTICS_ADMIN_TOKEN) from the container env
        # that gitops' k8s Secret injects (`envFrom`). Authored in nix and
        # evaluated to JSON here — the container has no `nix`, so the binary reads
        # the baked result, not the `.nix`. Without an explicit `--config` the
        # binary searches only XDG dirs + the prefixed `ANALYTICS_*`
        # env namespace — neither sees the bare-named Secret vars nor the `/data`
        # prod paths, so it would silently boot on dev defaults (empty maps key,
        # 127.0.0.1 bind that fails the k8s probe). This is what makes it correct.
        prodConfig = pkgs.writeText "config.json" (builtins.toJSON (import ./deploy/config.nix));
        containerStd = v_flakes.container.implement {
          inherit pkgs pname;
          containers."" = {
            port = pkgs.lib.toInt port;
            mounts = [ "/data" ];
            sqlite = [ "/data/app.db" ];
            healthPath = "/health";
            criticality = "normal";
            entrypoint = [ "${bin}/bin/analytics" "--config" "${prodConfig}" ];
            workingDir = "/data";
            imageEnv = [ "HOME=/data" ];
          };
        };
      in
      {
        # `nix run .#dev`         → MFE bundles (debug) + server, on port
        # `nix run .#test`        → cargo test (insta snapshots)
        # `nix run .#pull-prod-db`→ replace the local dev DB with prod's (litestream R2 restore)
        apps = {
          dev = { type = "app"; program = "${runDev}/bin/run-dev"; };
          default = { type = "app"; program = "${runDev}/bin/run-dev"; };
          test = { type = "app"; program = "${runTest}/bin/run-test"; };
          pull-prod-db = { type = "app"; program = "${runPullProdDb}/bin/pull-prod-db"; };
        };

        packages = {
          default = bin;
          inherit bin mfe;
        } // containerStd.packages;

        containers = containerStd.containers;

        devShells.default =
          with pkgs;
          mkShell {
            inherit stdenv;
            shellHook =
              ''
                if [ "$(nix config show lazy-trees 2>/dev/null)" != true ]; then
                  printf '%s\n' \
                    "✘ This repo requires Determinate Nix with lazy-trees=true." \
                    "  Stock nix produces flake.lock NAR hashes that diverge from CI (private inputs fail to verify)." \
                    "  Install: https://determinate.systems/nix   NixOS: nix.settings.lazy-trees = true" >&2
                  exit 1
                fi
              ''
              + pre-commit-check.shellHook
              + combined.shellHook
              + ''
                cp -f ${(v_flakes.files.treefmt) { inherit pkgs; }} ./.treefmt.toml
                cp -f ${(v_flakes.files.gitattributes) { inherit pkgs; lfs = false; }} ./.gitattributes
                ${dyldFallback}
              '';

            packages = [
              mold
              openssl
              pkg-config
              rust
              tailwindcss_4 # standalone Tailwind v4 CLI (dashboard.css + overview.css)
              wasm-bindgen-cli # must match wasm-bindgen =0.2.125
              litestream # manual restores/inspection; `.#pull-prod-db` wraps it
            ] ++ pre-commit-check.enabledPackages ++ combined.enabledPackages;

            env.RUST_BACKTRACE = 1;
            env.RUST_LIB_BACKTRACE = 0;
          };
      }
    );
}
