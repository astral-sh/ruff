# Codegen: AST ↔ contract ↔ target, lint-validated

`ruff_python_dto_check` already extracts route/handler facts from a Python AST.
This adds **codegen**: turn those facts into target-language source (handlers,
view templates, DTOs), with a **lint layer that validates the AST ↔ codegen ↔
template contract from the other end** — so when generation can't faithfully
represent the source, the gap is reported at the source instead of silently
producing wrong code.

The engine stays **generic and config-driven** (the crate was de-project-ified
on purpose). The first consumer is a Flask→Rust/axum port, but the same pipeline
must run against odoo and openproject next, so **no target- or project-specific
logic lives in the crate** — it lives in an extraction config + a target spec.

## The contract is the spine

A `RouteContract` is the shared interface all three sides agree on:

```text
RouteContract {
  id            // endpoint / function identity
  inputs        // path params (name,conv), query reads, form fields read
  data          // ORM/model references + query shape (filter/order/scope)
  output        // one of: Template{path, context_keys}
                //         Redirect{target}  | Json{shape}
                //         Blob{mime}        | Pdf{doc_kind}
  guards        // auth/tenant/permission predicates seen in the body
  provenance    // source path:line, raw body range
}
```

- **AST → contract**: extractors lift the contract out of the parsed body.
- **contract → target code**: the emitter renders a handler from the contract.
- **contract ↔ template**: `output.context_keys` is exactly the set the view
    template may reference; the lint checks both directions.

## Layers (all additive to the crate)

1. **Semantic extractors** (`extractors/body.rs`): walk the function body AST and
    fill the contract — call-sites (`render_template`, `redirect`, `jsonify`,
    `send_file`), model/query references, `request.form/args` reads, response
    kind, guard predicates. Driven by an **extraction profile** in config
    (call-name → fact), so odoo/openproject map their own conventions.
1. **Contract builder** (`contract.rs`): classify `output`/handler-kind from the
    facts (the 12 kinds are an emergent classification of `output` × `inputs`,
    not a hardcoded list). Emit the contract as JSON too (supersedes the external
    routing_table.json hand-tool).
1. **Target emitter** (`codegen/`): a **target spec** (TOML/JSON + text
    templates) maps each contract shape to source. Port the *proven* translation
    logic that already exists in the downstream repo's `tools/` — do NOT reinvent:
    - jinja→view-template translation from `tools/render_routes.py`
        (`_translate_cell_expr` + the rewriters: elif→else-if, `{{x or ''}}`,
        `strftime`→`format`, Option-aware `{% if let Some %}`).
    - DTO emission from `tools/contracts_to_rust.py` / `erp_models_to_dtos.py`.
        The emitter is target-pluggable; the first target is `rust-axum-seaorm`.
1. **Calibration lints** (`calibrate.rs`): check functions (ruff-style
    diagnostics) that validate the three-way contract and **enhance the pipeline
    at the source** when they fail:
    - every model/fact referenced in the AST appears in the emitted handler
        (no silent drop) — else `unmapped-model` / `dropped-fact`;
    - every template `context_key` is provided by the handler, and vice-versa —
        `template-context-mismatch`;
    - every `form_field_read` has a DTO field — `form-field-gap`;
    - output kind matches return type — `output-kind-mismatch`;
    - a fact the extractor couldn't classify → `extractor-gap` (points at the
        SOURCE layer to extend, not a downstream patch).
        Lints emit structured diagnostics (JSON + human), severity by whether the gap
        is a hard correctness risk or a TODO.

## CLI

`ruff-py-dto codegen --config <extract.json> --target <target.toml> --root <tree> --out <dir>`
emits, per route: contract JSON, target handler, target view template, DTO; plus
a `calibration.json` report. Re-runnable / idempotent. `harvest` is unchanged.

## Generality guardrails (for odoo / openproject)

- No hardcoded model names, template engines, or framework idioms in the crate.
- Extraction profile + target spec carry all project specifics.
- The 12 WoA handler-kinds are a *derived* classification; other codebases yield
    their own from the same `output × inputs` algebra.
- Golden tests per target under `tests/golden/codegen/`.

## First vertical slice (acceptance)

Implement contract + extractors + the `rust-axum-seaorm` target for two kinds
end-to-end — **`list_for_tenant`** and **`soft_delete`** — driven by the WoA
extraction profile, generating into a draft dir, with all four calibration lints
running and a golden test. The downstream per-kind Rust shapes are specced in
`woa-rs/port-drafts/<kind>/` (Sonnet drafts) — use them as the target's expected
output, then generalize the emitter so the other 10 kinds slot in by target-spec
entry, not new Rust per kind.

## Implemented modules

- `extractors/body.rs` — the semantic body walker + `ExtractionProfile`
    (config-driven call-name → fact map; Flask defaults; odoo/openproject supply
    their own profile). Produces `BodyFacts` (output kind, models, query/form
    reads, order-by, tenant-scope, mutation, soft-delete).
- `contract.rs` — `RouteContract` (the spine) + `HandlerKind` + the **priority
    classifier ported 1:1** from `classify_route_handlers.py::classify`. First
    match wins; the order is load-bearing.
- `codegen/target.rs` — `TargetSpec` (TOML or JSON; built-in
    `TargetSpec::rust_axum_seaorm()`). `ModelMapping::module_path` is the fragment
    between `models_root` and `::Model`. Carries an optional `templates_root` for
    jinja-column extraction (project-specific, never hardcoded).
- `codegen/jinja.rs` — jinja→askama *cell* translation ported from
    `render_routes.py` (`_translate_cell_expr`, elif→else-if, condition syntax,
    `{{x or ''}}`, `strftime`→`format`, Option-aware `{% if let Some %}`).
- `codegen/columns.rs` — jinja *table* extraction ported from
    `template_column_extract.py` (`find_table_block`, header/`<td>` cell pairing,
    outer-`{% else %}` empty-row detection, cell classification). Feeds the
    list/detail/sa-admin view emitters so they render the source columns.
- `codegen/dto.rs` — form-DTO emission, the struct shape from
    `contracts_to_rust.py` (each `inputs.form_fields` entry → `Option<String>`
    request-layer field; the calibration pass narrows the type against the DTO
    contract).
- `codegen/mod.rs` — the kind-generalized emitter (`KindRecipe`). All 12
    non-`other` `HandlerKind`s have an emitter arm (see coverage table below).
- `codegen/pipeline.rs` — the tree driver: parse → extract → contract → emit →
    calibrate → write (`contracts/`, `handlers/`, `views/`, `calibration.json`).
- `calibrate.rs` — the five calibration lints.

## Kind coverage (target `rust-axum-seaorm`)

All 12 emergent kinds emit end-to-end; `other` (the catch-all) stays a stub.

| kind                         | handler                                    | view               | DTO          | notes                                                           |
| ---------------------------- | ------------------------------------------ | ------------------ | ------------ | --------------------------------------------------------------- |
| `list_for_tenant`            | tenant-scoped list query                   | yes (real columns) | —            | jinja columns when `templates_root` set                         |
| `detail_for_tenant`          | scoped `find_by_id` + `ensure_tenant`      | yes                | —            | columns iff the detail page has a sub-table                     |
| `template_get`               | static render (+admin gate)                | yes (skeleton)     | —            | no model query                                                  |
| `soft_delete`                | scoped fetch → soft/hard delete → redirect | —                  | —            | `ActiveModel` `aktiv=Set(false)` for soft                       |
| `toggle_bool_field`          | scoped fetch → flip `aktiv` → redirect     | —                  | —            | soft_delete shape                                               |
| `get_redirect_shortcut`      | `Redirect`                                 | —                  | —            | `Option<CurrentUser>` for cond. redirect                        |
| `csrf_form_post_engine_call` | POST → form DTO → redirect                 | —                  | form DTO     |                                                                 |
| `form_get_post`              | `_get` render + `_post` handle             | yes (skeleton)     | form DTO     | two handlers                                                    |
| `ajax_json`                  | `Json<Response>`                           | —                  | response DTO | jsonify keys → fields                                           |
| `download_blob`              | `Response` bytes + Disposition             | —                  | —            | byte source is a documented stub                                |
| `pdf_render`                 | `Response` `application/pdf`               | —                  | —            | call-site shape; PDF API is a documented stub (NEVER `todo!()`) |
| `sa_admin_view`              | superadmin/admin gate + render             | yes                | —            | `sa_`-prefix → superadmin gate                                  |
| `signed_link_action`         | token-`Query` action (no `CurrentUser`)    | —                  | —            | separate auth stack; security-sensitive calibration             |
| `other`                      | documented stub                            | —                  | —            | not classified — extend the profile                             |

What odoo/openproject still need: their own `ExtractionProfile` (different
`render_call`/`query_attr`/markers), a `TargetSpec` with their model mappings +
`templates_root`, and — if their templates aren't jinja `<table>` based — a
template-shape extractor sibling to `columns.rs`. The walker, contract,
classifier, jinja cell translator, form-DTO emitter, and lints are unchanged.

## Model-path correctness (the systematic Sonnet-draft bug)

The drafts doubled flat model paths: `crate::models::customer::customer::Model`.
The correct path for a **flat** model is a single segment
(`crate::models::customer::Model`); **ERP** models are genuinely nested
(`crate::models::erp::k6_cash::cash_journal::Model`). The emitter encodes this
in `ModelMapping::module_path` (flat = `customer`; nested = `erp::k6_cash::…`),
and the golden test asserts the doubled form never appears. The
`unmapped-model` lint catches the inverse (a model the spec can't resolve).

## How a kind slots in (no new module per kind) — DONE for all 12

The recipe pattern that landed all 12 kinds:

1. Add the kind's `snake_case` id to the target spec's `emit_kinds`.
1. Add a `KindRecipe` variant + a single match arm in `codegen::emit` (the arm
    is a data-shaped `format!` like `emit_list_for_tenant` / `emit_soft_delete`;
    it reads the contract fields, not framework-specific globals).
    The simplest cases (`template_get`, `detail_for_tenant`) are near-copies of
    the list/delete emitters; `toggle_bool_field` is the `soft_delete` shape;
    the form-bearing kinds reuse `dto::emit_form_dto`. Kinds not listed in
    `emit_kinds` get a documented stub (never `todo!()` in a compiled path —
    PR #102 guardrail; the WoA pdf draft's `todo!()` is explicitly avoided).
1. The jinja cell translator (`jinja.rs`), the table extractor (`columns.rs`),
    the form-DTO emitter (`dto.rs`), and the model resolver are kind-agnostic
    and reused.

## Generality for odoo / openproject

- No model names, template engine, or framework idioms are hardcoded in the
    crate — they live in the `ExtractionProfile` (extraction) and the
    `TargetSpec` (mapping/recipes).
- The classifier reads neutral facts, so a different codebase yields its own
    kind distribution from the same `output × inputs` algebra.
- A new framework is a new `ExtractionProfile` (different `render_call`,
    `query_attr`, etc.) plus a new `TargetSpec`; the walker, contract, classifier,
    and lints are unchanged.
