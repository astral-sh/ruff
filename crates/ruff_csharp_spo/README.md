# ruff_csharp_spo

C# machine-plane frontend for `ruff_spo_triplet` — the C# sibling of
`ruff_cpp_spo` (C++/libclang) and the Python/Odoo + Ruby/Rails frontends.

It harvests an existing C# codebase into the **same** SPO triple graph every
other frontend emits, so a Rust transcode target can be generated and
parity-checked against the C# original — with the original application (or an
independent build of it) serving as the C# oracle.

## Why Roslyn (and why it's a separate process)

| Source language            | Parser in the family                         | Lives where                  |
| -------------------------- | -------------------------------------------- | ---------------------------- |
| Python (Odoo)              | `ruff_python_parser` / `ruff_python_ast`     | in-Rust                      |
| Ruby (Rails / OpenProject) | `ruff_ruby_spo`                              | in-Rust                      |
| C++ (Tesseract)            | `ruff_cpp_spo` via **libclang**              | in-Rust (FFI)                |
| **C#**                     | **Roslyn** (`Microsoft.CodeAnalysis.CSharp`) | **.NET tool** (`harvester/`) |

Roslyn *is* the C# compiler: it resolves base types, overrides, and member
types authoritatively, so it beats reparsing C# with a hand-written grammar
(e.g. a PEG library such as `ara3d/parakeet`, which is for *building* parsers,
not parsing C#). The cost is that Roslyn is .NET-only — there is no
Rust-callable binding — so unlike `ruff_cpp_spo`'s in-Rust `walk_tu`, the
**parse step runs as an out-of-process .NET tool** under `harvester/`. The two
halves meet at one seam: the ndjson `Triple` contract.

```text
C# corpus ──Roslyn harvester (.NET)──► triples.ndjson ──ruff_csharp_spo::load──►
    Vec<Triple> ──ruff_spo_triplet::reassemble / SPO store──► ClassView manifest
                                                                     │
                              C# original (the oracle) ──parity diff─┘
```

## Run it

```sh
# 1. Harvest the C# source to ndjson (needs a .NET 8 SDK):
dotnet run --project crates/ruff_csharp_spo/harvester/CSharpSpoHarvester.csproj \
  -- /path/to/csharp-src triples.ndjson

# Some codebases use a bespoke ADO.NET DAL instead of EF Core: mutators
# follow an `add_*`/`del_*` naming convention via a DAL field
# (`main.mysql.add_x(...)`), so the `calls` fact needs the
# naming-convention flags rather than (or in addition to) the default
# EF-Core `--mutator-names` set:
dotnet run --project crates/ruff_csharp_spo/harvester/CSharpSpoHarvester.csproj \
  -- /path/to/csharp-src triples.ndjson \
  --mutator-prefixes add_,del_,update_,insert_ --mutator-receivers mysql

# 2. Load + validate from Rust (the load IS the validation):
#    let triples = ruff_csharp_spo::load(&fs::read_to_string("triples.ndjson")?)?;
```

### CLI flags

All optional; defaults reproduce the original EF-Core-flavoured behaviour, so
existing invocations are unaffected.

| flag                           | default                                                       | what it does                                                                                                                                                                                                                                                                      |
| ------------------------------ | ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--ns <name>`                  | `csharp`                                                      | IRI namespace prefix for every subject/object                                                                                                                                                                                                                                    |
| `--mutator-names a,b,c`        | the EF Core set (`SaveChanges`, `Update`, `Add`, `Remove`, …) | exact method names that make a `calls` fact fire — **replaces** the default set when given                                                                                                                                                                                        |
| `--mutator-prefixes add_,del_` | none                                                          | method-name *prefixes* that also count as mutators — for bespoke ADO.NET DALs with a naming convention instead of a fixed method set                                                                                                                                              |
| `--mutator-receivers mysql`    | none (any receiver)                                           | restricts the `calls` fact to invocations whose receiver's last identifier segment is in this list — `main.mysql.add_x(...)` matches receiver `mysql`; a form's own `set_Foo(...)` does not. **Applies to every mutator match**, name-set or prefix-based alike, once set. |

A name match is `--mutator-names` (exact) **OR** `--mutator-prefixes` (prefix)
— either is sufficient; `--mutator-receivers`, when non-empty, then further
restricts by receiver.

`load` (a thin wrapper over `ruff_spo_triplet::from_ndjson`) rejects malformed
lines **and** any predicate outside the closed `ruff_spo_triplet::Predicate`
vocabulary, returning a `ParseError` that names the line and offending
predicate. A harvester bug therefore surfaces as a hard error at load time,
never as silent drift into the store — so a clean `Ok(_)` is itself the schema
guarantee, with no separate post-load check to run.

## Predicate mapping (scaffold)

The scaffold walks the **syntax layer** and emits the structural facts every
class carries. Subjects/objects are namespaced with the `--ns` prefix
(`csharp:` by default).

| C# construct                             | SPO triple                                                                                                                        |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `class Invoice`                          | `(csharp:Invoice, rdf:type, ogit:ObjectType)`                                                                                     |
| `: DbBase` / `: IFoo`                    | `(csharp:Invoice, inherits_from, csharp:DbBase)`                                                                                  |
| `string number { get; set; }` / field    | `(csharp:Invoice, has_field, csharp:Invoice.number)` + `(…​.number, rdf:type, ogit:Property)` + `(…​.number, field_type, string)`   |
| `void Save()`                            | `(csharp:Invoice, has_function, csharp:Invoice.Save)` + `(…​.Save, rdf:type, ogit:Function)`                                       |
| `static` method                          | `(csharp:Invoice.Save, is_static, true)`                                                                                          |
| `int Foo(int x, string y)`               | `(csharp:Invoice.Foo, returns_type, int)` + `(…​.Foo, has_param_type, "0:int")` + `(…​.Foo, has_param_type, "1:string")`            |
| method access specifier (always present) | `(csharp:Invoice.Foo, has_visibility, "public"\|"protected"\|"private")`                                                          |

All predicates above are in the closed vocabulary already (shared with the
C++/Rails frontends). NARS truth is `(f=1.0, c=0.9)` — the declared/structural
provenance tier.

## Body-fact arm (DTO arm)

`EmitBodyArm` (`harvester/Program.cs`) populates the fuzzy-recipe-codebook
fingerprint (`ruff/.claude/knowledge/fuzzy-recipe-codebook.md` §2) — the same
`writes_field` / `reads_field` / `raises` / `calls` / `writes_if_blank`
predicates the Ruby/Rails frontend emits, syntax-only (a bare `X` is
heuristically a member read/write; only `this.X` reads are tracked — a
SemanticModel upgrade would prune locals/params and pick up bare-identifier
reads too). **Syntax-only, SemanticModel upgrade pending; the `writes`/`raises`/
`writes_if_blank` facts are certain-by-construction, `reads`/`calls` are
Inferred** (matching the Ruby frontend's provenance split). Helpers are ✅ via
`has_visibility` — the private/protected split the recipe codebook needs to
separate the public adapter surface from internal hooks.

Tested: `harvester/fixtures/recipe_shapes.cs` exercises all 7 recipe
centroids (Default via `??=`, Default via `if (x == null)`, Normalize,
Compute, Cascade, Guard, Compensate, WriteRaise) plus the
`--mutator-prefixes`/`--mutator-receivers` configurability, built and run with
`dotnet-sdk-8`. `ruff_csharp_spo`'s unit tests round-trip a sample of every
predicate the arm can emit, including the signature plane. The arm has also
run end-to-end against a real production C# corpus (~97k triples;
`ruff_csharp_spo::load` validates all of them; that corpus's bespoke
`add_*`/`del_*`-via-DAL-receiver convention needs
`--mutator-prefixes add_,del_,update_,insert_ --mutator-receivers mysql`).

## SemanticModel upgrade (next step, not in the scaffold)

The scaffold uses `CSharpSyntaxTree.ParseText` (no build required). To resolve
symbols — fully-qualified base types, `virtually_overrides` targets, and
ORM/attribute bindings (`maps_to_collection`, field→column-name) — upgrade
`harvester/` to:

1. add `Microsoft.CodeAnalysis.CSharp.Workspaces` + `Microsoft.Build.Locator`,
1. `MSBuildWorkspace.OpenSolutionAsync(YourApp.sln)`,
1. walk with the per-document `SemanticModel` (`GetDeclaredSymbol` /
    `GetSymbolInfo`) instead of raw syntax.

For forms-over-data CRUD apps, the *valuable* predicates skew toward data
shape (class → collection/table, field → column) and form→route, rather than
the deep virtual-override graph `ruff_cpp_spo` harvests from Tesseract. Any
predicate not yet in `ruff_spo_triplet::Predicate` is a deliberate ontology
addition there first (a new enum variant + `as_str` / `from_str` arm), then
emitted here — `load` is the gate that forces that order: emit a predicate
before adding it to the vocabulary and the next load fails with a
`ParseError` naming it.

## Provenance / non-vendoring

C# source corpora stay **upstream** — never vendored into this repo. The
harvester reads a path you point it at; it ships no C# sources.
