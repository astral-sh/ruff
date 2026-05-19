# `woa_transcode_harvest`

Per-route transcode bundle harvester for the `AdaWorldAPI/WoA` Python source →
`AdaWorldAPI/woa-rs` Rust port.

**This crate does not affect `ruff` or `ty`.** It depends on `ruff_python_parser`,
`ruff_python_ast`, `ruff_source_file`, and `ruff_text_size` — i.e. it consumes
ruff's parser/AST as a library. All other ruff crates are unchanged.

## Why it lives in this repo

Ruff's parser is the production-grade Python parser this fork already maintains.
Building the harvester here means:

- One toolchain, one CI for both ruff lints and WoA transcode bundles.
- We inherit upstream Astral parser improvements without rebasing a separate fork.
- Ruff continues to work as a normal linter on WoA Python.

## Spec

- RFC: `AdaWorldAPI/woa-rs:rfcs/v02-005-ruff-transcode-harvester.md`
- Schema: `AdaWorldAPI/woa-rs:rfcs/v02-005-bundle-schema.md`

## Quickstart

```bash
cargo run -p woa_transcode_harvest --bin woa-transcode -- \
    harvest-one --rel woa/blueprints/vorgaenge_ops.py ../WoA/woa/blueprints/vorgaenge_ops.py
```

Full repo harvest:

```bash
cargo run -p woa_transcode_harvest --bin woa-transcode -- \
    harvest ../WoA --out ../woa-rs/vendor/woa-transcode-bundles
```

## Status

Phase 0 — identity + decorators raw. Phases 1–5 in RFC-v02-005.
