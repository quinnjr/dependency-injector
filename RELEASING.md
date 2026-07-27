# Releasing

Releases are **tag-driven**: pushing a `vX.Y.Z` tag to GitHub kicks off every publish
workflow. Nothing needs to be published from a laptop.

## 1. Prepare the release

1. **Bump versions** (all in one commit):
   - [Cargo.toml](Cargo.toml) — the root `version`, *and* the
     `dependency-injector-derive` dependency version a few lines below it
   - [dependency-injector-derive/Cargo.toml](dependency-injector-derive/Cargo.toml)
   - Binding manifests:
     - [ffi/nodejs/package.json](ffi/nodejs/package.json) (`version`)
     - [ffi/python/pyproject.toml](ffi/python/pyproject.toml) (`project.version` — the
       wheel workflow builds with whatever is in this file)
     - [ffi/python/dependency_injector/__init__.py](ffi/python/dependency_injector/__init__.py)
       (`__version__` — not derived from `pyproject.toml`; bump it by hand)
     - [ffi/csharp/DependencyInjector/DependencyInjector.csproj](ffi/csharp/DependencyInjector/DependencyInjector.csproj)
       (`<Version>` — the NuGet workflow also re-stamps this from the tag at build time)
     - The Go module needs no manifest bump; it is versioned by the git tag itself.
2. **Update [CHANGELOG.md](CHANGELOG.md)** (Keep a Changelog format, one section per version).
3. **Merge to `main`.** The `release.yml` validate job fails if the tag version does not
   match the root `Cargo.toml` version, so tag only after the bump has landed on `main`.
4. **Wait for the `bindings-test` CI job to be green on the release commit** before any
   binding package (npm, PyPI, NuGet) is published — whether by a tag workflow or by
   `scripts/deploy-ffi.sh`. This gate exists because a koffi ownership bug once shipped
   in a binding release without the code ever having been executed.

## 2. Tag and push

```bash
git checkout main && git pull
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z
```

## 3. What the tag triggers

| Workflow | What it does |
|----------|--------------|
| [release.yml](.github/workflows/release.yml) | Validates tag == `Cargo.toml` version, runs `cargo test --all-features` + clippy, publishes to **crates.io** in order — `dependency-injector-derive` first, then `dependency-injector`, skipping any version already published — and creates the **GitHub Release** with a changelog grouped from conventional commits. |
| [build-native.yml](.github/workflows/build-native.yml) | Builds the FFI `cdylib` for **5 platforms** (Linux x64/arm64, macOS x64/arm64, Windows x64) and attaches the libraries plus a **SHA256SUMS** manifest to the GitHub Release. |
| [build-python-wheels.yml](.github/workflows/build-python-wheels.yml) | Builds platform wheels + sdist for `dependency-injector-rust`, publishes to **PyPI**, and attaches them to the release. |
| [build-nuget.yml](.github/workflows/build-nuget.yml) | Packs `PegasusHeavy.DependencyInjector` with bundled native runtimes, pushes to **NuGet.org**, and attaches the `.nupkg` to the release. |

After the Release workflow completes, the new
[verify-release.yml](.github/workflows/verify-release.yml) runs automatically
(`workflow_run`, also manually triggerable): it verifies the new version is actually
installable from crates.io (`cargo add` + build, with retries for indexing lag) and that
all five native libraries are attached to the GitHub Release.

### Known gap in the tag flow

- The npm package `@pegasusheavy/dependency-injector` is **not** published by any tag
  workflow today; `scripts/deploy-ffi.sh` is still the only path that runs `pnpm publish`.

## Legacy scripts

[scripts/deploy.sh](scripts/deploy.sh), [scripts/publish.sh](scripts/publish.sh), and
[scripts/deploy-ffi.sh](scripts/deploy-ffi.sh) are **legacy, local release paths** that
pre-date the tag-driven workflows above and are superseded by them. They are kept only
for local dry-runs (`--dry-run` flags) and are slated to be replaced by
[release-plz](https://release-plz.dev/) — see [ROADMAP.md](ROADMAP.md).
