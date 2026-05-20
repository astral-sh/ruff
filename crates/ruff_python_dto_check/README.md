# `ruff_python_dto_check`

A config-driven extractor over `ruff_python_parser`. Point it at a Python
source tree, get JSON bundles describing the structured facts your config
asks for — decorated routes, class-based views, DTO-shaped classes, CLI
command surfaces, or anything else that can be matched by AST shape.

A `preflight` subcommand scans a fresh tree once and proposes a config
file from what it finds: top decorator patterns, framework fingerprint,
file-naming conventions, sibling-relative anomalies. The intent is that a
new contributor (or a Claude Code session) can clone the repo, run
preflight on their codebase, and have a useful starting config in under a
minute.

**This crate is additive to `ruff` and `ty`.** It depends on
`ruff_python_parser`, `ruff_python_ast`, `ruff_source_file`, and
`ruff_text_size` — i.e. it consumes ruff's parser/AST as a library. All
other ruff crates are unchanged.

## Why a separate crate in this repo

Ruff's parser is a production-grade Python parser, this repository
already maintains it, and the extractor benefits from upstream parser
improvements without a separate clone. Keeping it in-tree means one
toolchain, one CI configuration, and ruff continues to work as a normal
linter and formatter.

## Quickstart

Point at a single file:

```bash
cargo run -p ruff_python_dto_check --bin ruff-py-dto -- \
    harvest-one --rel app/blueprints/views.py path/to/views.py
```

Harvest a whole tree using a config:

```bash
cargo run -p ruff_python_dto_check --bin ruff-py-dto -- \
    harvest --config examples/flask.config.json --root path/to/project --out ./bundles
```

Propose a config from a tree:

```bash
cargo run -p ruff_python_dto_check --bin ruff-py-dto -- \
    preflight path/to/project --out ./preflight-out
```

See [`examples/flask.config.json`](examples/flask.config.json) for the
canonical Flask example, and
[`schemas/ruff-py-dto.config.schema.json`](schemas/ruff-py-dto.config.schema.json)
for the full config schema.

## Output layout

```
out/
├── bundles/
│   ├── <family>.ndjson     — one JSON object per line, one per matched function
│   └── ...
├── indices/
│   ├── by_decorator_stack.json
│   ├── by_ast_hash.json
│   └── by_family.json
└── manifest.json
```

Bundles are grouped by `family` (typically derived from the source
filename stem) so a reader can inspect adjacent routes within one file
without cross-tree noise. The `indices/` files surface global structure:
the decorator-stack index groups handlers by their decorator signature
(singletons in a stack of size > 1 self-flag as anomalies), and
`by_ast_hash.json` groups functions with identical AST shape (near-dup
detection without calling anything a duplicate).

## Scope

Current implementation:

- One matcher kind: `function_with_decorator`
- Flask-style route detection via `@<expr>.route(...)` in the default profile
- Config-driven emit fields (`def.name`, `def.params`, `def.body.source`,
  `decorator.args[0]`, `decorator.kwargs.<name>`, etc.)
- Per-family observation block with set-algebra comparisons and percentile
  distributions (no advisory English strings — pure content encoding)
- Preflight subcommand with decorator histogram + framework fingerprint

Out of scope for this iteration (reserved for future work):

- `class_with_base` and `module_attribute_call` matchers
- Full name resolution via `ruff_python_semantic`
- Output formats other than NDJSON + JSON indices
