# Contributing to Lemmata

Thanks for helping. Bug reports, fixes for install instructions and code are all welcome.

## Getting started

See [README.md](README.md#developing) for the toolchain. Then:

```bash
pnpm install
pnpm dev
```

Before opening a pull request, run what CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check
pnpm test
cargo build -p lemmata-cli && scripts/check-fixtures.sh target/debug/lemmata
```

## Where things live

[docs/PLAN.md](docs/PLAN.md) explains the architecture. In short: Rust crates under `crates/` do the work (TeX, logs, SyncTeX, history, spelling, bibliography), `apps/desktop/src-tauri` exposes them as commands, and `apps/desktop/src` is the Svelte interface.

## Easy first contributions

- **Install commands** for your distro in [data/install.toml](data/install.toml) and [data/distro-packages.toml](data/distro-packages.toml). No Rust needed.
- **Error explanations**: add a plain-language hint for a TeX error in `classify_error` in [crates/lemmata-log/src/lib.rs](crates/lemmata-log/src/lib.rs), with a real log excerpt under `tests/fixtures/logs/` and a test.
- **Templates** in [templates/](templates/). Each needs a `template.toml` and must compile with the recommended TeX Live set.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:` …). The changelog is generated from them.

## Licence

By contributing you agree that your work is released under the [GPL-3.0](LICENSE).
