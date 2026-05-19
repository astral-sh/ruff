# `ruff_python_dto_check`

Config-driven extractor over `ruff_python_parser`. Point it at a Python source
tree, get JSON bundles describing the structured facts you asked for —
decorated routes, class-based views, DTO-shaped classes, CLI command surfaces,
or anything else that can be matched by AST shape.

A `preflight` subcommand scans a fresh tree once and proposes a config file
from what it finds: top decorator patterns, framework fingerprint, file-naming
conventions, sibling-relative anomalies. Other Claude Code sessions (and
humans) can clone this fork, run preflight on their codebase, and have a
useful starting config in under a minute.

**This crate does not affect `ruff` or `ty`.** It depends on
`ruff_python_parser`, `ruff_python_ast`, `ruff_source_file`, and
`ruff_text_size` — i.e. it consumes ruff's parser/AST as a library. All other
ruff crates are unchanged.

## Why it lives in this repo

Ruff's parser is a production-grade Python parser, this fork already
maintains it, and the extractor benefits from upstream parser improvements
without a separate clone. Keeping the extractor in-tree means one toolchain,
one CI, and ruff continues to work as a normal linter.

## History

Landed as `woa_transcode_harvest` (Flask-and-WoA-specific harvester) in
PR #1. Renamed and being generalized so any consumer — any Python framework,
any reading Claude session — can use it.

## Status

Renamed; generic config-driven refactor in progress. Phase-0 Flask harvest
behavior (decorator-based route detection) still works as the default
extractor profile and is exercised by the golden test.

## Quickstart (current behavior, Flask harvest profile)

```bash
cargo run -p ruff_python_dto_check --bin ruff-py-dto -- \
    harvest-one --rel woa/blueprints/vorgaenge_ops.py ../WoA/woa/blueprints/vorgaenge_ops.py

cargo run -p ruff_python_dto_check --bin ruff-py-dto -- \
    harvest ../WoA --out ../woa-rs/vendor/woa-transcode-bundles
```
