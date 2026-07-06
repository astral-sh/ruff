# `.claude/knowledge/` — curated methods for the SPO/transcode side of ruff

Each doc teaches a **method** (the fishing), not a one-off answer, and carries a
`READ BY:` header naming which sessions/agents must load it before producing
output in that domain. Grep here before re-deriving.

| doc | teaches | READ BY |
|---|---|---|
| `fuzzy-recipe-codebook.md` | cook a `(verb, criteria)` recipe codebook from imperative method-body facts; correlate fuzzy bodies to declarative recipes; roll the buckets until the residue is irreducible; collect the jitter | any session harvesting body facts (`ruff_{ruby,python,csharp,cpp}_spo`) or lowering the OGAR DO-arm; the `fuzzy-proposer` agent |

Agents live in `../agents/`. The `fuzzy-proposer` card is the operator for
`fuzzy-recipe-codebook.md`.

## The SPO frontends and their body-fact "DTO arm" status (2026-07-06)

The recipe codebook runs on the `(writes, reads, raises, calls)` quartet +
private-helper split on `ruff_spo_triplet::Function`. Only `ruff_ruby_spo`
emits the full arm today; **C#, C++, and Python still need it** (pure AST-walk
work per frontend, zero shared-IR change — the predicates already `expand()`).
See `fuzzy-recipe-codebook.md` §2 for the coverage table and how to add the arm.
