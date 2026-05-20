# `ruff_python_dto_check` — Generic Refactor Design Spec

> Authored 2026-05-19 as the locked spec for the post-rename refactor.
> Branch: `claude/woa-transcoding-inventory-rSrhm` on `AdaWorldAPI/ruff`.
> Baseline: `origin/main @ 5ff5be9` (merged PR #1: Phase-0 Flask harvester).
> Rename commit: `9eba995` (woa_transcode_harvest → ruff_python_dto_check).
> Worker contract: single Sonnet worker, isolation=worktree, sentinel token below.

---

## §1 Status & Baseline

Crate is currently a **Flask-specific route harvester** (originally
`woa_transcode_harvest`). It hardcodes:

- `@<expr>.route(...)` decorator detection (`src/extractors/routes.rs`)
- `family` inferred from filename stem (`src/lib.rs:family_from_path`)
- Verb classification of decorator strings (`src/extractors/decorators.rs`)
- Bundle layout with WoA-shaped fields (`schema_version`, `endpoint`,
  `phase`, `complexity_score`, `body_loc`, `body_sha256`).

286 source lines, 1 golden test (`tests/golden_test.rs` — `wo_list`).
`cargo check -p ruff_python_dto_check --all-targets` is clean.
`cargo test -p ruff_python_dto_check` passes.

---

## §2 Target

**Generic config-driven extractor over `ruff_python_parser`.** Point it at
a Python source tree, get JSON bundles describing the structured facts the
config asks for — decorated routes, class-based views, DTO-shaped classes,
CLI command surfaces, anything matchable by AST shape.

Audience is **other sessions of this user, cloning this fork**. Ergonomics
priority: a fresh session running `ruff-py-dto preflight ./some-repo` gets
a usable starting config in under a minute.

Behavior of the current Flask harvester is **preserved as the default
profile**: existing `wo_list` golden test must keep passing, ideally without
modification, with the worker using a checked-in `examples/flask.config.json`.

---

## §3 Locked Design

### §3.1 Config Schema

JSON config, must fit on a screen for the simple case. Canonical example
(committed at `examples/flask.config.json`):

```jsonc
{
  "$schema": "../schemas/ruff-py-dto.config.schema.json",
  "root": ".",
  "include": ["woa/blueprints/**/*.py"],
  "exclude": ["**/tests/**", "**/__pycache__/**", "**/.venv/**"],
  "match": [
    {
      "id": "flask_route",
      "kind": "function_with_decorator",
      "decorator": {
        "attribute": "route",
        "min_positional_args": 1
      },
      "emit": {
        "url":             "decorator.args[0]",
        "methods":         "decorator.kwargs.methods",
        "function_name":   "def.name",
        "signature":       "def.params",
        "body_source":     "def.body.source",
        "decorators_all":  "def.decorators"
      }
    }
  ],
  "group": {
    "family_from_filename": {
      "regex": "^(?P<family>[a-z_]+?)(?:_ops|_bp|_routes)?\\.py$"
    }
  }
}
```

Schema notes:

- `match` is a **list of rules**, not a single rule. One pass over the tree
  matches all rules. Bundles carry `match_id` so consumers can demux.
- `kind` values for this iteration: `function_with_decorator` only.
  `class_with_base` and `module_attribute_call` are explicitly **out of
  scope for this PR** (reserve the field for forward compat).
- `decorator` selector: any of `{attribute: "<name>"}`, `{name: "<name>"}`,
  `{any: true}`. If multiple set, narrowest wins.
- `emit` values are dot-path expressions into the matched AST node's
  context. Supported paths for `function_with_decorator`:
  - `def.name`, `def.params`, `def.body.source`, `def.decorators`,
    `def.range.line_start`, `def.range.line_end`
  - `decorator.args[<int>]`, `decorator.kwargs.<name>`, `decorator.raw`
  - Path is emitted as `null` if absent (no error). Type-mismatches
    (e.g. `decorator.kwargs.methods` is not a list) emit the raw value
    and a `_type_note` sibling field — never `panic!`.
- `group.family_from_filename.regex` must contain a named capture `family`.
- Unknown top-level keys are **rejected** at config parse with a
  `did_you_mean: <closest known key>` hint.

Schema file ships at `schemas/ruff-py-dto.config.schema.json` (JSON Schema
Draft 2020-12). Schema is the source of truth; the deserializer validates
against it. Failure mode: pretty-printed validation error with the path.

### §3.2 Preflight Subcommand

```
ruff-py-dto preflight <root>   [--out <path>]
```

Scans the tree once and emits a starter config + a candidate-miss report.
Two output sections (separated by a blank line if stdout, two files if
`--out <dir>`):

**Section 1 — proposed config.** Same JSON shape as §3.1, with inline
comments (jsonc) explaining each filled-in field and the evidence count.
Example: `// "attribute": "route" — matched 653 decorators in tree; runners-up: get=4, post=3`.

**Section 2 — preflight report** (JSON file `preflight.report.json`).
Content-encoded only — no advisory English strings. Fields:

```jsonc
{
  "tree_stats": {
    "py_files_scanned": 1247,
    "py_files_parseable": 1241,
    "py_files_failed_parse": 6,
    "total_function_defs": 4892,
    "total_class_defs": 312
  },
  "framework_fingerprint": {
    "imports_seen": {"flask": 38, "fastapi": 0, "django.urls": 0, "starlette": 0, "pyramid": 0}
  },
  "decorator_histogram": {
    "by_attribute_name":  {"route": 653, "before_request": 14, "errorhandler": 8, "cli.command": 3, "...": "..."},
    "by_full_pattern":    {"bp.route": 641, "app.route": 12, "router.get": 4, "...": "..."}
  },
  "first_arg_url_test": {
    "decorator_pattern": "bp.route",
    "string_literal_count": 641,
    "name_reference_count": 0,
    "starts_with_slash_count": 641
  },
  "decorator_co_occurrence": {
    "with_bp.route": {"require_tenant": 488, "audit_log": 421, "cache(60)": 32}
  },
  "filename_convention": {
    "files_with_matched_routes": 51,
    "stem_suffix_histogram": {"_ops": 38, "_bp": 8, "_routes": 3, "(none)": 2}
  },
  "url_template_segments": {
    "<tenant>": 4, "<int:tenant_id>": 7, "<uuid:org_id>": 0, "<int:customer_id>": 41
  },
  "body_string_scan_hits": {
    "g.tenant":         123,
    "current_app.tenant":  4,
    "request.tenant":     17,
    "Tenant.query":       12
  },
  "candidate_misses": {
    "defs_with_decorators_but_unmatched": 29,
    "examples": [
      {
        "file": "woa/blueprints/admin_ops.py",
        "function": "rotate_secret_keys",
        "decorators_raw": ["@admin_only", "@bp.cli.command('rotate-keys')"]
      },
      "..."
    ]
  },
  "add_url_rule_findings": [
    {"file": "...", "line": 142, "expr": "bp.add_url_rule('/orders', view_func=OrderListView.as_view('orders'))"}
  ],
  "register_blueprint_graph": [
    {"parent": "app", "child": "billing_bp", "url_prefix": "/billing"},
    "..."
  ]
}
```

These are the **deeper-drilling heuristics** for catching nested patterns
(class-based views, `add_url_rule`, blueprint nesting, tenant-via-URL
templates, tenant-via-body-access). The next session reading this report
sees the structure of what was found and naturally drills into the
candidate-miss examples.

### §3.3 Matchers

Worker implements a single matcher kind for this PR: `function_with_decorator`.
Module structure:

```
src/
├── lib.rs                — public API, top-level types
├── config.rs             — Config struct + deserializer + schema validator
├── matcher/
│   ├── mod.rs            — Matcher trait + dispatch
│   └── function_with_decorator.rs
├── emit.rs               — dot-path expression evaluator over matched context
├── bundle.rs             — Bundle output type (kept; reshape per §3.5)
├── observations.rs       — comparison_within_family computation
├── preflight/
│   ├── mod.rs            — preflight CLI handler
│   └── scanner.rs        — single-pass collector for all preflight signals
├── extractors/           — REMOVE (folded into matcher/ + preflight/)
└── bin/
    └── ruff_py_dto.rs    — CLI (harvest | preflight | harvest-one)
```

Worker MAY keep the existing `extractors/decorators.rs` if and only if
the verb-classification logic moves into a clearly-named utility module
(e.g. `src/heuristics/verb_from_method.rs`) and is only used by the Flask
profile's golden test for backward-compatible behavior. Otherwise delete.

### §3.4 Content-Encoded Observations

Each emitted bundle carries a `comparison_within_family` block. Pure
content, **no advisory English strings**. Set algebra + distributions only:

```jsonc
{
  "match_id": "flask_route",
  "file": "woa/blueprints/vorgaenge_ops.py",
  "function_name": "wo_list",
  // ... emit fields per config ...
  "comparison_within_family": {
    "family": "vorgaenge",
    "family_size": 13,
    "decorators_family_intersection": ["require_tenant", "audit_log", "bp.route"],
    "self_minus_family_intersection": [],
    "family_intersection_minus_self": [],
    "body_lines_self": 27,
    "body_lines_family_distribution": {"p25": 18, "p50": 22, "p75": 31, "p95": 60, "p99": 412, "max": 412},
    "param_count_self": 2,
    "param_count_family_distribution": {"p25": 1, "p50": 2, "p75": 3, "max": 14},
    "ast_hash_self": "sha256:abc123...",
    "ast_hash_family_collisions": []
  }
}
```

**Forbidden in this block:**
- Any string that is not a structural identifier or a hash digest
- Fields named `warning`, `issue`, `smell`, `outlier`, `confidence`,
  `severity`, `recommendation`, `should_*`, `is_anomalous_*`
- Booleans whose name editorializes (`is_too_long`, `looks_dead`).
  `internal_callers: 0` is allowed (plain count); `is_dead: true` is not.

The numbers and sets do the asserting. A reader scrolling past sees
`family_intersection_minus_self: ["require_tenant"]` and *can* react;
the file doesn't tell them they should.

### §3.5 Output Layout

```
out_dir/
├── bundles/
│   ├── <family-1>.ndjson           — one JSON object per line, one per match in family
│   ├── <family-2>.ndjson
│   └── ...
├── indices/
│   ├── by_decorator_stack.json     — { "decorator_sig (sorted, sep=|)": ["endpoint", ...], ... }
│   ├── by_ast_hash.json            — { "<hash>": ["endpoint-a", "endpoint-b"] }   (groups of size ≥ 2 only)
│   └── by_family.json              — { "<family>": ["endpoint", ...] }
└── manifest.json                   — {schema_version, ruff_py_dto_version, generated_at, root, config_path, totals: {...}}
```

`bundles/<family>.ndjson` replaces the previous `out/<family>/<function>.json`
layout. Reason: a family file is what a reader naturally compares within;
adjacent routes on adjacent lines makes outliers visible by layout alone.
Stable ordering inside each ndjson: by `function_name` ascending.

`indices/` are pure content-encoded observations at the repo level.
Singleton groups in `by_decorator_stack.json` self-flag as outliers
without any boolean. AST-hash collisions surface near-duplicates without
calling them "duplicates" anywhere.

### §3.6 CLI Surface

```
ruff-py-dto harvest    --config <path> [--out <dir>] [--root <override>]
ruff-py-dto preflight  <root> [--out <dir>]
ruff-py-dto harvest-one --config <path> --rel <repo-rel> <file.py>
ruff-py-dto schema     [--out <path>]    — write JSON Schema to <path> or stdout
```

`harvest --out` default: `./ruff-py-dto-out`. `preflight --out` default:
stdout (concatenated proposed-config + report JSON, blank-line separated).
`--root` override lets a config be committed in `examples/` and pointed at
a different tree on each invocation.

---

## §4 Bundle Ownership

The worker owns (read-write):

```
crates/ruff_python_dto_check/Cargo.toml
crates/ruff_python_dto_check/README.md
crates/ruff_python_dto_check/DESIGN.md              (this file — updates only as RFC)
crates/ruff_python_dto_check/schemas/               (new dir)
crates/ruff_python_dto_check/examples/              (new dir)
crates/ruff_python_dto_check/src/                   (full rewrite of extractors/ tree)
crates/ruff_python_dto_check/tests/                 (extend golden_test; add config_parse_test, preflight_smoke_test, observation_test)
```

The worker reads-only:

```
crates/ruff_python_parser/**                        (parser API surface)
crates/ruff_python_ast/**                           (AST types)
crates/ruff_source_file/**                          (LineIndex)
crates/ruff_text_size/**                            (Ranged trait)
AGENTS.md                                           (ruff repo coding rules)
Cargo.toml                                          (root workspace — read only)
```

The worker **must not touch** any other crate, the root `Cargo.toml`
beyond verifying the workspace glob picks up the rename, or any file
under `crates/ruff*/` outside the dto_check crate.

---

## §5 Implementation Phases (worker may commit per phase)

**Phase A — Schema + Deserialization.** `config.rs`, `schemas/`, basic
`Config::from_path` + `Config::validate`. Unit test: parse the canonical
Flask config, reject unknown keys with `did_you_mean`, reject missing
named-capture in `family_from_filename`.

**Phase B — Matcher.** `matcher/function_with_decorator.rs` + `emit.rs`
dot-path evaluator. Replaces the current `extractors/routes.rs`. Single
test: parse `tests/golden/wo_list.input.py` with `examples/flask.config.json`,
assert the emit fields match expectations. **wo_list_identity test from
the current `tests/golden_test.rs` must still pass** (adapted to the
config-driven path, same assertions on the emitted values).

**Phase C — Bundle reshape + per-family ndjson + indices.** `bundle.rs`,
`observations.rs`, `lib.rs` orchestration. Tests: family ndjson is
well-formed and stable-ordered; observation block has no forbidden field
names; index files group correctly.

**Phase D — Preflight.** `preflight/scanner.rs` + `preflight/mod.rs` + bin
wiring. Smoke test: run preflight against `tests/golden/wo_list.input.py`
(synthetic single-file tree), assert the proposed config matches a
checked-in expected output, and the report JSON has the expected stats.

Commits: one per phase, message format
`[dto-check] phase-<A|B|C|D>: <one-line>` with the body quoting the
DESIGN.md section satisfied.

---

## §6 Acceptance Gates

Worker self-validates before reporting done. PP-13 brutally-honest-tester
re-runs all of these post-impl.

| Gate | Command | Verdict |
|---|---|---|
| Compile | `cargo check -p ruff_python_dto_check --all-targets` | clean |
| Lints | `cargo clippy -p ruff_python_dto_check --all-targets --all-features -- -D warnings` | clean |
| Format | `cargo fmt --check -p ruff_python_dto_check` | clean |
| Tests | `cargo test -p ruff_python_dto_check` | all pass |
| Behavior preserved | `wo_list_identity` test passes under `examples/flask.config.json` | identical emitted fields |
| Schema validates | `examples/flask.config.json` validates against `schemas/ruff-py-dto.config.schema.json` | green |
| No advisory strings | `grep -rE '"(warning\|issue\|smell\|outlier\|confidence\|recommendation\|severity\|should_\|is_too_\|looks_)"' crates/ruff_python_dto_check/src` | zero hits |
| Workspace untouched outside crate | `git diff --name-only origin/main..HEAD` | only paths under `crates/ruff_python_dto_check/` (+ `Cargo.lock` is acceptable) |

---

## §7 Anti-Patterns (Auto-Reject)

Adopted from ruff `AGENTS.md` + this design:

- `unwrap()`, `panic!`, `unreachable!` outside `#[cfg(test)]`
- `anyhow::Result` in public lib API (binary may use it)
- Re-introducing Flask-specific code paths in `src/` (it lives in
  `examples/flask.config.json` only)
- Any string in `comparison_within_family` or `preflight.report.json`
  that is advisory rather than structural (see §3.4 forbidden list)
- New workspace dependencies without justification in the commit message
- Touching files outside the §4 ownership table
- Comments narrating obvious code (per ruff AGENTS.md)
- Suppressing clippy with `#[allow(...)]` — use `#[expect(...)]` per
  ruff AGENTS.md if absolutely necessary, with a one-line justification

---

## §8 Reading Protocol

Worker **must** read these files at depth `full` before writing code,
and emit proof-of-read (SHA-256 + line count) in its initial reply:

1. This file (`crates/ruff_python_dto_check/DESIGN.md`) — full
2. `crates/ruff_python_dto_check/src/lib.rs` — full (the baseline being replaced)
3. `crates/ruff_python_dto_check/src/extractors/routes.rs` — full
4. `crates/ruff_python_dto_check/src/extractors/decorators.rs` — full
5. `crates/ruff_python_dto_check/tests/golden_test.rs` — full
6. `crates/ruff_python_dto_check/tests/golden/wo_list.input.py` — full
7. `AGENTS.md` (ruff root) — full

Worker **must** replay sentinel token verbatim in first reply (LD-1 test):

```
SENTINEL: RUFF-DTO-CHECK-REFACTOR-WAVE-7q3m
```

---

## §9 Done Criteria

- All §6 acceptance gates pass on the worker's final commit
- All files in §4 ownership table exist
- Original `wo_list_identity` test passes (adapted, but same assertions)
- New tests added per §5 phase-end test lines
- Zero entries in `.claude/board/REQUESTS-FROM-AGENTS.md` open for this agent
- Worker writes a single AGENT_LOG.md entry (German, per board convention)
  pointing to this DESIGN.md and listing per-phase commit SHAs

---

## §10 If Stuck

Per `worker-template.md`: stop, write to
`/home/user/woa-rs/.claude/board/REQUESTS-FROM-AGENTS.md` with blocker
type (`AMBIGUITY` | `MISSING_INVARIANT` | `EXTERNAL_DEPENDENCY` |
`SPEC_SOURCE_MISMATCH`), and idle on the file. Orchestrator answers in
`ANSWERS-TO-AGENTS.md`. **Do not guess.**

Specifically anticipated ambiguities:

- **Schema validation crate.** No new workspace dep without RFC. Use
  `jsonschema` if already in `Cargo.lock` workspace; otherwise hand-roll
  validation against the parsed schema struct in `config.rs` and file a
  request before adding a dep.
- **AST hash function.** Define as: post-order walk of the `Suite`,
  emitting a string per node of `<NodeKind>(<child-count>)`, SHA-256 of
  the concatenation. Document in `observations.rs` doc-comment. Do not
  add a hash crate unless `sha2` is already in workspace.
- **Body line count.** Defined as `range.line_end - range.line_start + 1`.
- **Percentile algorithm.** Use linear interpolation between order
  statistics (nearest-rank if family size < 4).
