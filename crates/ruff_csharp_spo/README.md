# ruff_csharp_spo

C# machine-plane frontend for `ruff_spo_triplet` — the C# sibling of
`ruff_cpp_spo` (C++/libclang) and the Python/Odoo + Ruby/Rails frontends.

It harvests an existing C# codebase (MedCare first) into the **same** SPO
triple graph every other frontend emits, so a transcode target (`medcare-rs`)
can be generated and parity-checked against the C# original — with
[`AdaWorldAPI/MedCareV2`](https://github.com/AdaWorldAPI/MedCareV2) as the
independent C# oracle.

## Why Roslyn (and why it's a separate process)

| Source language | Parser in the family | Lives where |
| --- | --- | --- |
| Python (Odoo) | `ruff_python_parser` / `ruff_python_ast` | in-Rust |
| Ruby (Rails / OpenProject) | `ruff_ruby_spo` | in-Rust |
| C++ (Tesseract) | `ruff_cpp_spo` via **libclang** | in-Rust (FFI) |
| **C# (MedCare)** | **Roslyn** (`Microsoft.CodeAnalysis.CSharp`) | **.NET tool** (`harvester/`) |

Roslyn *is* the C# compiler: it resolves base types, overrides, and member
types authoritatively, so it beats reparsing C# with a hand-written grammar
(e.g. a PEG library such as `ara3d/parakeet`, which is for *building* parsers,
not parsing C#). The cost is that Roslyn is .NET-only — there is no
Rust-callable binding — so unlike `ruff_cpp_spo`'s in-Rust `walk_tu`, the
**parse step runs as an out-of-process .NET tool** under `harvester/`. The two
halves meet at one seam: the ndjson `Triple` contract.

```text
MedCare (C#) ──Roslyn harvester (.NET)──► triples.ndjson ──ruff_csharp_spo::load──►
    Vec<Triple> ──ruff_spo_triplet::reassemble / SPO store──► ClassView manifest
                                                                     │
                              MedCareV2 (C# oracle) ──parity diff────┘
```

## Run it

```sh
# 1. Harvest the C# source to ndjson (needs a .NET 8 SDK):
dotnet run --project crates/ruff_csharp_spo/harvester/CSharpSpoHarvester.csproj \
  -- /path/to/MedCare triples.ndjson

# 2. Load + validate from Rust:
#    ruff_csharp_spo::load(&fs::read_to_string("triples.ndjson")?)?
#    ruff_csharp_spo::unknown_predicates(&triples)  // must be empty
```

`load` rejects malformed lines; `unknown_predicates` names any predicate
outside the closed `ruff_spo_triplet::Predicate` vocabulary — a harvester bug
must surface there, never as silent drift into the store.

## Predicate mapping (scaffold)

The scaffold walks the **syntax layer** and emits the structural facts every
class carries. Subjects/objects are `medcare:`-namespaced.

| C# construct | SPO triple |
| --- | --- |
| `class Patient` | `(medcare:Patient, rdf:type, ogit:ObjectType)` |
| `: DbBase` / `: IFoo` | `(medcare:Patient, inherits_from, medcare:DbBase)` |
| `string kdnr { get; set; }` / field | `(medcare:Patient, has_field, medcare:Patient.kdnr)` + `(…​.kdnr, rdf:type, ogit:Property)` + `(…​.kdnr, field_type, string)` |
| `void Save()` | `(medcare:Patient, has_function, medcare:Patient.Save)` + `(…​.Save, rdf:type, ogit:Function)` |
| `static` method | `(medcare:Patient.Save, is_static, true)` |

All predicates above are in the closed vocabulary already (shared with the
C++/Rails frontends). NARS truth is `(f=1.0, c=0.9)` — the declared/structural
provenance tier.

## SemanticModel upgrade (next step, not in the scaffold)

The scaffold uses `CSharpSyntaxTree.ParseText` (no build required). To resolve
symbols — fully-qualified base types, `virtually_overrides` targets, and
MedCare's MongoDB `db_*.cs` attribute bindings (`maps_to_collection`,
field→BSON-name) — upgrade `harvester/` to:

1. add `Microsoft.CodeAnalysis.CSharp.Workspaces` + `Microsoft.Build.Locator`,
2. `MSBuildWorkspace.OpenSolutionAsync(MedCare.sln)`,
3. walk with the per-document `SemanticModel` (`GetDeclaredSymbol` /
   `GetSymbolInfo`) instead of raw syntax.

MedCare is a WinForms + MongoDB CRUD app, so the *valuable* predicates skew
toward data shape (`db_*.cs` class → collection, field → column) and
form→route, rather than the deep virtual-override graph `ruff_cpp_spo` harvests
from Tesseract. Any predicate not yet in `ruff_spo_triplet::Predicate` is a
deliberate ontology addition there first (a new enum variant + `as_str` /
`from_str` arm), then emitted here — `unknown_predicates` is the gate that
forces that order.

## Provenance / non-vendoring

C# source corpora (MedCare, MedCareV2) stay **upstream** — never vendored into
this repo. The harvester reads a path you point it at; it ships no C# sources.
