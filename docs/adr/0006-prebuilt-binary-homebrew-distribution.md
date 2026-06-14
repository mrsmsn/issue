# 6. Distribute prebuilt binaries via Homebrew (not source-build)

- Date: 2026-06-14
- Status: Accepted (supersedes the source-build formula shipped with v0.1.0)
- Deciders: maintainer
- Related: `docs/adr/0005-tui-lazyissue-and-workspace-split.md`,
  `.github/workflows/release.yml`, `.github/templates/issue.rb`

## Context

The first release (v0.1.0) used a **source-build** Homebrew formula: `brew install`
ran `cargo install` on the user's machine, compiling the workspace (incl.
ratatui/crossterm/notify, ~1–2 min) and requiring a Rust toolchain. That works, but
installs are slow and pull a build toolchain. The reference setup the maintainer
likes (`mrsmsn/darwinvpn`, Go + GoReleaser) instead ships **prebuilt** per-arch
tarballs as release assets, so `brew install` is a fast download + extract.

## Decision

Ship prebuilt binaries. On each `v*` tag, CI (`macos-14`) cross-builds the two
workspace binaries for **both macOS arches** and uploads tarballs + checksums to the
GitHub Release; the formula downloads the right one and installs it — no compilation,
no `depends_on "rust"`.

- Assets (GoReleaser-style names): `issue_<ver>_darwin_arm64.tar.gz`,
  `issue_<ver>_darwin_amd64.tar.gz`, `checksums.txt`. Each tarball contains the
  `issue` and `lazyissue` binaries at top level.
- x86_64 is cross-compiled from the Apple-Silicon runner via the universal macOS SDK
  (`cargo build --target x86_64-apple-darwin`); verified locally that `notify`'s C
  deps cross-compile cleanly.
- The formula uses `on_macos { on_arm { … } on_intel { … } }` with per-arch
  `url` + `sha256`, then `bin.install "issue"`/`"lazyissue"`.
- **Completions still auto-install**: `generate_completions_from_executable` runs the
  just-installed native binary, so `issue <Tab>` keeps working with no `source` line.

## Alternatives considered

- **Source-build (the v0.1.0 approach).** Simplest formula and platform-agnostic, but
  slow installs and a Rust build dep. Rejected for the default experience; can still
  be installed from `HEAD`/source manually.
- **cargo-dist** (the Rust analog of GoReleaser). Generates CI + installers + the
  formula. Powerful, but adds a tool and config; the hand-rolled matrix is small and
  keeps the existing template-render-and-push flow.
- **arm64-only prebuilt.** Smaller CI, but excludes Intel Macs. Cross-building amd64
  is one extra target + tarball, so we include both.

## Consequences

- `brew install mrsmsn/tap/issue` is now download-only (no toolchain, fast).
- CI does more: cross-build ×2, package, upload assets, then render/push the formula.
  The tag-must-equal-Cargo-version guard and the `cargo test` gate are unchanged.
- The formula now carries `version` + two `sha256`s, all filled at release time.
- Adding Linux later = add `x86_64-unknown-linux-gnu`/`aarch64-unknown-linux-gnu`
  targets (likely a Linux runner) + `on_linux` blocks.
