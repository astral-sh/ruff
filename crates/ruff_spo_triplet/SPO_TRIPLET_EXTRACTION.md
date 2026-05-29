# SPO Triplet Extraction — methodology & cross-language reuse guide

> **Audience:** anyone wiring a new source-language frontend (e.g. the
> OpenProject Ruby/Rails extraction) onto the shared SPO triplet core.
>
> **TL;DR:** parse your AST → fill a `ModelGraph` → call `expand()` →
> write ndjson. The triple vocabulary, truth calibration, and IRI shape
> are fixed in `ruff_spo_triplet`; you only write the AST→IR step.

---

## 1. What this is and why it exists

Business logic in an ORM-backed app (Odoo, Rails, Django, …) is a graph:
**entities** own **fields** and **methods**; methods **compute** fields,
**depend on** other fields, **read** fields, **raise** errors, and
**traverse** relations. That graph is the same shape regardless of the
host language — only the syntax that expresses it differs.

`ruff_spo_triplet` captures that shape once as a closed set of **SPO
triples** (Subject–Predicate–Object) with NARS `(frequency, confidence)`
truth values. The Odoo (Python) extraction and the OpenProject
(Ruby/Rails) extraction both emit **byte-identical** triple shapes, so a
single downstream consumer (`lance_graph`'s SPO store, the Foundry-shape
`action_emitter`, the `link_chain` splitter) works on either without
modification.

```
   Python AST ─┐
               ├─► ModelGraph (IR) ─► expand() ─► Vec<Triple> ─► ndjson ─► SPO store
   Ruby AST  ──┘        ▲                  ▲            ▲
                  language-specific   shared core   shared core
                  (you write this)   (this crate)  (this crate)
```

The reuse seam is the `ModelGraph` IR. Everything below the IR is shared;
everything above it is the per-language frontend.

---

## 2. The triple schema (closed vocabulary)

Nine triple forms over seven predicates. `ns` is the namespace prefix you
choose for the source app (`odoo`, `openproject`, …).

| predicate            | subject            | object             | provenance     | meaning |
| ---                  | ---                | ---                | ---            | --- |
| `rdf:type`           | `ns:model`         | `ogit:ObjectType`  | Structural     | this name is an entity |
| `rdf:type`           | `ns:model.field`   | `ogit:Property`    | Structural     | this name is a field |
| `rdf:type`           | `ns:model.fn`      | `ogit:Function`    | Structural     | this name is a method |
| `has_function`       | `ns:model`         | `ns:model.fn`      | Structural     | entity owns method |
| `emitted_by`         | `ns:model.field`   | `ns:model.fn`      | Authoritative  | method writes field |
| `depends_on`         | `ns:model.field`   | `ns:model.<dep>`   | Authoritative  | field's declared compute deps |
| `reads_field`        | `ns:model.fn`      | `ns:model.field`   | Inferred       | method body reads field |
| `raises`             | `ns:model.fn`      | `exc:<Type>`       | Authoritative  | method raises error |
| `traverses_relation` | `ns:model.fn`      | `ns:model.<rel>`   | Inferred       | method walks relation |

**IRI shape.** Subjects and objects are `"<ns>:<model>.<member>"`. The
single dot separates model from member; dotted *dependency paths*
(`line_ids.balance`) are emitted **verbatim** under the model IRI
(`odoo:account_move.line_ids.balance`) and split into per-hop link
triples later by the downstream `link_chain` splitter — the extractor
stays source-faithful and does no path resolution.

**`ogit:` is the canonical OGIT vocabulary** (`http://www.purl.org/ogit/`),
not a project-local namespace. Don't invent `https://…/ObjectType`.

### Provenance → truth (the NARS calibration)

| tier            | `(f, c)`      | when |
| ---             | ---           | --- |
| `Structural`    | `(1.0, 1.0)`  | true by construction (a name *is* a model/field/method; ownership) |
| `Authoritative` | `(0.95, 0.90)`| declared or directly observed in body (`@api.depends`, a `raise`, the field a compute assigns) |
| `Inferred`      | `(0.85, 0.75)`| heuristic from body shape (an attribute read, a loop-target relation) |

The downstream store gates queries by NARS *expectation*, so a strict
query can drop `Inferred` edges and keep only declared facts. The tier is
load-bearing — pick it honestly per edge. `Predicate::default_provenance()`
gives the calibrated default; override per-edge only when your frontend
can *prove* a stronger tier (e.g. a Rails frontend that statically
resolves a read can promote `reads_field` to `Authoritative`).

---

## 3. The IR you fill (`ModelGraph`)

```rust
pub struct ModelGraph { pub namespace: String, pub models: Vec<Model> }
pub struct Model    { pub name: String, pub fields: Vec<Field>, pub functions: Vec<Function> }
pub struct Field    { pub name: String, pub depends_on: Vec<String>, pub emitted_by: Option<String> }
pub struct Function { pub name: String, pub reads: Vec<String>, pub raises: Vec<String>, pub traverses: Vec<String> }
```

That's the entire contract. Plain owned data, no behaviour. Fill it from
your AST, hand it to `expand()`.

### Naming rule

Keep `Model::name` as the source names it, with ONE normalisation: if the
host uses dots in model names (Odoo `account.move`), convert them to
underscores (`account_move`) so the IRI dot is unambiguously the
model↔member separator. Rails class names (`WorkPackage`) have no dots —
use them as-is.

---

## 4. The query this enables ("a + b → c through d?")

The reason for the graph: answer *"which field `c` does method `d` emit
when inputs `a` and `b` change?"* as a deterministic graph deduction, not
a similarity search:

```text
  { c : (c depends_on a) ∧ (c depends_on b) }   then   { d : (c emitted_by d) }
```

Two reverse `depends_on` lookups intersected, then one `emitted_by`
lookup. This is what makes the extracted ontology a *compute graph*
(Foundry-shape) rather than a flat list of routes. The
`lance_graph::graph::spo::action_emitter` composes per-method
`ActionSpec { effects, inputs, raises, reads, traverses }` records
straight off these edges.

---

## 5. Writing a new frontend — the Ruby/Rails (OpenProject) guide

Five steps. Only step 2 is real work.

### Step 1 — pick a Ruby parser

Options, cheapest first:

- **`lib-ruby-parser`** (Rust crate, pure Rust, no Ruby runtime) — best
  fit for a Rust frontend; gives you a typed AST. *Recommended.*
- **tree-sitter-ruby** (via the `tree-sitter` crate) — robust, lossy on
  some semantics but great for structural sweeps.
- Shell out to Ruby's own `ripper`/`parser` gem and read s-expressions —
  only if you already have a Ruby toolchain in the loop.

A scaffold crate (`ruff_ruby_spo`, see §6) is provided wired for
`lib-ruby-parser` with `todo!()` markers at each extraction point.

### Step 2 — map Rails constructs to the IR

This is the whole job. The mapping (mirror of the Odoo column in the
cheat-sheet in `src/ir.rs`):

| IR target               | Rails / ActiveRecord source |
| ---                     | --- |
| `Model::name`           | `class WorkPackage < ApplicationRecord` → `WorkPackage` |
| `Field::name`           | DB columns (from `db/schema.rb`), `attribute :x`, `attr_accessor`, `store_accessor` |
| `Field::depends_on`     | association chains a derived attribute reads (`time_entries.hours`); if you parse `schema.rb` you can also seed column→column deps |
| `Field::emitted_by`     | a memoized/derived method that assigns the attribute (`def total_hours; @total_hours ||= …; end`) |
| `Function::name`        | instance methods (`def compute_total_hours`) |
| `Function::reads`       | `self.x` reads and bare attribute reads in the method body |
| `Function::raises`      | `raise X`, `errors.add(...)`, and `validates`/`validate` callbacks (treat the validation as a guard that raises `ActiveRecord::RecordInvalid`) |
| `Function::traverses`   | association walks in the body (`children.each`, `time_entries.map`, `project.members`) — the association name is the relation |

Notes specific to Rails:

- **Associations are your relations.** `belongs_to :project`,
  `has_many :time_entries` declare the traversable relations. A method
  body that calls `time_entries` is traversing `time_entries`. Seed the
  set of valid relation names from the association declarations so you can
  distinguish a relation walk from an ordinary method call.
- **Validations are guards.** `validates :subject, presence: true` and
  `validate :custom_check` are the Rails analogue of Odoo's
  `@api.constrains` + `raise`. Emit them as `raises exc:ActiveRecord::RecordInvalid`
  (Authoritative) on the validating method, or on a synthetic
  `_validate` function for declarative `validates`.
- **`exc:` namespace is shared.** Ruby exception class names keep their
  `::` (`exc:ActiveRecord::RecordInvalid`) — the `exc:` prefix is the same
  one Odoo uses (`exc:UserError`). Don't translate; just prefix.
- **Callbacks (`before_save`, `after_create`) → functions** whose
  `traverses`/`reads`/`raises` you extract from the referenced method.

### Step 3 — build the `ModelGraph`

```rust
let mut graph = ModelGraph::new("openproject");
for class in rails_classes {
    let mut model = Model::new(normalise(&class.name));
    model.fields = extract_fields(&class);       // step 2
    model.functions = extract_functions(&class); // step 2
    graph.models.push(model);
}
```

### Step 4 — expand + write

```rust
use ruff_spo_triplet::{expand, to_ndjson};
let triples = expand(&graph);            // sorted, de-duplicated, truth-weighted
std::fs::write("openproject.spo.ndjson", to_ndjson(&triples))?;
```

### Step 5 — load downstream (already built, no new work)

The ndjson loads directly into `lance_graph::graph::spo::odoo_ontology::load_ontology`
(rename or generalise that loader's name; the *format* is identical).
`action_emitter::emit_actions` and `link_chain::split_all_depends_on`
then work on the OpenProject graph exactly as they do on Odoo's.

---

## 6. The scaffold crate (`ruff_ruby_spo`)

`crates/ruff_ruby_spo/` is a compiling skeleton:

- depends on `ruff_spo_triplet`,
- exposes `extract(source_tree: &Path) -> ModelGraph`,
- has `todo!()` bodies at each of the step-2 extraction points with a
  doc-comment naming the exact Rails construct to read,
- ships a unit test that builds a hand-written `ModelGraph` and asserts
  the `expand()` output — so the *target shape* is locked even before the
  parser is wired.

Start there: replace the `todo!()`s one predicate at a time, running the
locked-shape test after each. When all are filled, point it at the
OpenProject `app/models/` tree.

---

## 7. Verifying parity with the Odoo extraction

Two graphs are "the same shape" if, for a structurally-equivalent input,
they produce the same predicate histogram and the same truth tiers. The
crate's own tests pin this:

- `triple::tests::provenance_truth_tiers_match_odoo_calibration`
- `expand::tests::truth_tiers_are_assigned_per_predicate`
- `integration_tests::two_model_graph_round_trips_through_ndjson`
  (uses a Rails-shaped `ModelGraph`)

When you wire the Ruby frontend, add a fixture test that runs a small
real OpenProject model through `extract()` + `expand()` and asserts the
expected `ActionSpec` shape downstream. That closes the loop: same IR
contract → same triples → same Foundry-shape actions.

---

## 8. Pointers

- `src/triple.rs` — the closed vocabulary (`Predicate`, `EntityKind`, `Provenance`).
- `src/ir.rs` — the `ModelGraph` contract + the Odoo↔Rails cheat-sheet.
- `src/expand.rs` — the deterministic IR→triples projection.
- `src/ndjson.rs` — the on-disk format (matches the `lance_graph` loader).
- Downstream consumers (in the `lance-graph` repo):
  `crates/lance-graph/src/graph/spo/odoo_ontology.rs` (loader),
  `…/action_emitter.rs` (Foundry `ActionSpec` composer),
  `…/link_chain.rs` (dotted-path splitter).
