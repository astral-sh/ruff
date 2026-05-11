# Setuptools editable finder history

This note records the history of the setuptools-generated editable finder that
`ty_setuptools_editable` parses. It is intentionally about emitted setuptools
artifacts, not hypothetical future formats.

Research date: 2026-05-11.

Source checkout used for this note:

- repository: `pypa/setuptools`
- cloned `main` HEAD: `84ed5913724df5a12dc804e1d5efe12508e706d2`
- HEAD date: 2026-04-13
- nearest tag description: `v82.0.1-16-g84ed59137`

The important conclusion is:

> The current parser matches the finder artifact emitted by setuptools
> `v70.0.0` and later, but older real setuptools finder artifacts had materially
> different source structure. If we ever decide to support editable installs
> already present on disk from pre-`v70` setuptools, the parser will need an
> explicit compatibility expansion.

## Supported compatibility target

The crate's compatibility target is intentionally:

> import-hook editable finder artifacts emitted by setuptools `v70.0.0` and
> later.

That is narrower than “all released PEP 660 editable finder artifacts.” It
matches the first released setuptools format that uses the top-level annotated
`MAPPING` / `NAMESPACES` declarations parsed by this crate, along with the
namespace fallback rules modeled by `ty_module_resolver`.

This cutoff is deliberate:

- it describes a concrete emitted artifact family, not a vague “latest
    setuptools” claim
- it keeps parsing literal and auditable
- it avoids partially accepting older artifacts while silently modeling their
    runtime semantics as if they were the `v70+` finder

The `v64.0.0+` alternative is documented below as a future expansion path, not
as a current support promise.

## What setuptools emits today

Current setuptools still has three editable-install strategies in
`setuptools/command/editable_wheel.py`:

1. `_StaticPth`
    - emits `__editable__.<name>.pth`
    - the file contains plain path entries
1. `_LinkTree`
    - emits the same `.pth` shape, pointing at a generated auxiliary tree
1. `_TopLevelFinder`
    - emits both:
        - `__editable__.<name>.pth`
        - `<safe finder module>.py`

Only the third strategy is the subject of this crate.

The current launcher line is still:

```python
import __editable___pkg_finder; __editable___pkg_finder.install()
```

The current generated finder module has these data declarations:

```python
MAPPING: dict[str, str] = {...}
NAMESPACES: dict[str, list[str]] = {...}
PATH_PLACEHOLDER = ...
```

and two cooperating import helpers:

- `_EditableFinder`, appended to `sys.meta_path`
- `_EditableNamespaceFinder`, reached through a path hook and a synthetic
    placeholder entry

The present resolver behavior in `ty_module_resolver` now mirrors the current
setuptools semantics that matter most:

- top-level exact mappings are handled by the editable meta finder
- immediate children of a mapped parent are delegated through `PathFinder`
- deeper descendants are expected to flow through the parent package path
- editable namespace placeholders participate before appended editable meta
    finders
- empty namespace path lists reuse `MAPPING[fullname]`

## How often the source changed

I counted changes from the cloned setuptools history with:

```sh
git log --follow -- setuptools/command/editable_wheel.py
git log --follow -G '(_FINDER_TEMPLATE|MAPPING|NAMESPACES|PATH_PLACEHOLDER|_EditableFinder|_EditableNamespaceFinder|def _finder_template|finder\.install\(\)|sys\.meta_path|sys\.path_hooks|sys\.path\.append)' -- setuptools/command/editable_wheel.py
```

Results:

| Measure                                                            |      Count |
| ------------------------------------------------------------------ | ---------: |
| Commits touching `editable_wheel.py` since the PEP 660 work landed |        121 |
| Finder-template-related commits by the conservative regex above    |         12 |
| Last finder-structure change found in cloned `main`                | 2024-03-07 |

The broad file churn is front-loaded:

| Year                     | All commits touching `editable_wheel.py` | Finder-template-related commits |
| ------------------------ | ---------------------------------------: | ------------------------------: |
| 2022                     |                                       49 |                               6 |
| 2023                     |                                       27 |                               5 |
| 2024                     |                                       36 |                               1 |
| 2025                     |                                        9 |                               0 |
| 2026 through cloned HEAD |                                        0 |                               0 |

So the file as a whole has remained active, but the generated finder skeleton
appears structurally stable from the March 2024 change through the April 2026
HEAD I inspected.

## Timeline of real finder forms

### 1. Scaffolded meta-finder template

- commit: `1a531db35955`
- date: 2022-04-09
- first containing tag: `v63.0.0b1`

This commit introduced `_FINDER_TEMPLATE` itself. The template was not yet in
the shape used today:

- finder class name: `__EditableFinder`
- `MAPPING` and `NAMESPACES` lived as class attributes
- both declarations were plain assignments, not annotated assignments
- the module self-installed by executing `__EditableFinder.install()`
- `NAMESPACES` was not yet the modern dict-of-path-lists contract

This is useful as source history, but the next commit is the better starting
point for the first fully emitted artifact.

### 2. First emitted finder sidecar plus `.pth` launcher

- commit: `994ca214c`
- date: 2022-04-09
- first containing tag: `v63.0.0b1`

This commit made `_TopLevelFinder` actually write:

- a sibling finder module named from `__editable__.<dist>.finder`

- a `.pth` file containing:

    ```python
    import <finder>; <finder>.install()
    ```

Important shape details:

- finder module names were already sanitized into the familiar
    `__editable___..._finder` spelling
- the `.pth` launcher form visible today originates here
- the finder still used:
    - class-local `MAPPING = ...`
    - class-local `NAMESPACES = ...`
    - namespace values represented as a set-like collection, not the later
        dict-of-lists representation

`finder_module_from_pth_line` is historically well aligned with this launcher
form. The `.pth` line has remained stable across the later commits I inspected.

### 3. Namespace path-hook architecture

- commit: `3c71c872d`
- date: 2022-04-17
- first containing tag: `v63.0.0b1`

This is the large structural pivot toward the modern architecture.

Changes:

- `MAPPING` and `NAMESPACES` moved from class attributes to module-level globals
- `NAMESPACES` became a dict from module name to path list
- `PATH_PLACEHOLDER` appeared
- `_EditableNamespaceFinder` appeared
- `install()` began registering a `sys.path_hooks` entry and appending the
    placeholder to `sys.path`

This form is semantically much closer to current setuptools, but it still uses
plain assignments:

```python
MAPPING = {...}
NAMESPACES = {...}
```

Our current parser intentionally does not parse this variant, because it looks
only for top-level annotated assignments.

Two small follow-ups landed the same day:

- `501aec9d4`
    - extracted `_paths()`
    - added `find_module()` to satisfy the path-entry finder interface
- `468724337`
    - skipped path-hook registration when `NAMESPACES` is empty

### 4. Case-sensitivity experiments and nested-lookup churn

Several changes in mid-2023 changed the runtime lookup semantics without
changing the basic data declarations:

- `877af7b3e` on 2023-07-28
- `ad1d39ac0` on 2023-07-30
- `db3743a90` on 2023-08-02

The key outcome was `db3743a90`, which switched nested lookup toward
`PathFinder` to avoid hand-rolling filesystem case behavior.

This matters for `ty_module_resolver`, not because it changes what this parser
extracts, but because it changes how the extracted mapping should be interpreted.
The current resolver should follow the later setuptools semantics, not the older
recursive path-building experiments.

### 5. Immediate-child-only mapping semantics

- commit: `bc82e28d7`
- date: 2023-08-17
- first containing tag: `v68.1.1`

This commit established the current core rule:

- exact top-level mappings are handled directly
- only immediate children of a mapped parent are delegated to `PathFinder`
- deeper nesting should be handled later through normal import machinery using
    the parent path

This is the historical source of the review feedback that required ty to probe a
parent mapping before accepting a retained nested exact mapping.

### 6. Legacy namespaces expand `MAPPING`

- commits:
    - `681894831`
    - `d6513447b`
- date: 2023-09-06
- first containing tag: `v68.2.0`

`681894831` briefly threaded an `extra_path` list through the immediate-child
`PathFinder` call. It was empty in the checked-in code and later disappeared.

`d6513447b` is more important: setuptools started merging legacy namespace
packages into `MAPPING`, not just ordinary package roots. That widened the
semantic meaning of a mapping entry while preserving the on-disk data shape.

For ty, this means “a mapping entry” should not be mentally restricted to an
ordinary top-level package discovered from `packages`; setuptools can place
legacy namespace roots there too.

### 7. Current annotated data format and empty-namespace fallback

- commit: `19b63d1b8`
- date: 2024-03-07
- first containing tag: `v70.0.0`

This is the compatibility boundary for the current parser.

The generated finder changed from:

```python
MAPPING = {...}
NAMESPACES = {...}
```

to:

```python
MAPPING: dict[str, str] = {...}
NAMESPACES: dict[str, list[str]] = {...}
```

The same commit also changed namespace path construction in two semantically
important ways:

- when `NAMESPACES[fullname]` is empty and `fullname in MAPPING`, the finder
    reuses that mapping path
- it always appends the placeholder path, including nested namespace cases

Those two details are exactly the current setuptools behavior behind:

- empty namespace path lists resolving through `MAPPING`
- editable namespace placeholders outranking later appended editable meta
    finders

### 8. No later finder-structure change found

From `19b63d1b8` through cloned `main` HEAD
`84ed5913724df5a12dc804e1d5efe12508e706d2`, I did not find another structural
mutation to the finder template under the conservative identifier-based history
search above.

Later commits continued to edit `editable_wheel.py` for typing, cleanup, build
flow, messages, and surrounding implementation details, but not the emitted
finder wire format that this crate currently parses.

## What the current parser assumes

`crates/ty_setuptools_editable/src/lib.rs` currently makes these assumptions.

| Parser assumption                                                                 | Historical status                                                                                 | What would break it                                                                                     |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `.pth` launcher is `import <finder>; <finder>.install()`                          | Stable since the first emitted finder in `994ca214c`                                              | A different import form, extra executable suffix text, or non-leading `import`                          |
| Finder module name starts with `__editable___` and is simple identifier-like text | Stable for the emitted strategy I inspected                                                       | A renamed finder convention or a non-simple imported expression                                         |
| `MAPPING` is a top-level annotated assignment                                     | True only from `19b63d1b8` / `v70.0.0` onward                                                     | Pre-`v70` real artifacts, a rollback to plain assignment, or moving the assignment inside another scope |
| `NAMESPACES` is a top-level annotated assignment                                  | True only from `19b63d1b8` / `v70.0.0` onward                                                     | Same as above                                                                                           |
| `MAPPING` literal is a dict of string keys to string paths                        | Consistent with the real finder families inspected                                                | Replacing the dict literal with a constructor/helper expression, or non-string path expressions         |
| `NAMESPACES` literal is a dict of string keys to list-of-string paths             | True for modern finder families; earliest emitted forms used a different namespace representation | Tuple/set values, helper expressions, or old pre-path-hook forms                                        |

One subtle but important point:

- the parser ignores the annotation *text*
- it only needs the AST node to be `AnnAssign`

So changing `dict[str, str]` to another type annotation spelling would not by
itself break parsing. Removing the annotation entirely would.

## Real historical tweaks that would break or mislead us

### Break parsing outright

These are not hypothetical parser fears; they correspond to real historical
setuptools forms that the current parser does not accept:

1. **Plain assignments before `v70.0.0`**
    - historical source used `MAPPING = ...` and `NAMESPACES = ...`
    - current parser scans only annotated assignments
1. **Class-local data declarations in the earliest emitted finder**
    - the first emitted sidecar kept `MAPPING` and `NAMESPACES` on
        `__EditableFinder`
    - current parser scans only the module suite
1. **Old namespace value shapes**
    - the first emitted finder used a namespace collection that was not the
        current dict-of-lists contract
    - current parser expects a dict whose values are list literals

If ty ever wants compatibility with editables produced by old setuptools
versions already sitting in site-packages, these are the concrete extensions to
consider first.

### Parse successfully but emulate the wrong runtime

Some changes do not alter the extractable literals, but they do alter the import
semantics ty must model:

1. **The switch to immediate-child-only lookup**
    - `bc82e28d7`
    - parent-path probing matters before retained nested exact mappings
1. **Legacy namespaces entering `MAPPING`**
    - `d6513447b`
    - mapping entries are not limited to ordinary top-level roots
1. **Empty namespace path fallback and permanent placeholder retention**
    - `19b63d1b8`
    - missing these rules produces resolver results that differ from Python

These are the commits that matter most for the resolver layer above this parser.

## Compatibility boundary for ty

The current implementation is a deliberate “current setuptools” parser:

- it is well matched to the emitted `_TopLevelFinder` artifact from
    `v70.0.0` onward
- it intentionally does not accept older real variants
- its most brittle dependency is not the `.pth` launcher, which has been stable,
    but the top-level annotated-literal layout of `MAPPING` and `NAMESPACES`

That boundary is defensible if the goal is:

> understand the artifact setuptools writes today, without widening into every
> historical formatter.

It is not sufficient if the goal becomes:

> understand any editable finder artifact that may already exist in user
> environments, regardless of the setuptools version that wrote it.

Those are different compatibility promises and should stay explicit.

## If we later want `v64.0.0+`

Setuptools `v64.0.0` is the first released PEP 660 editable-install release, so
`v64.0.0+` is the natural broader compatibility target if we decide ty should
understand old editable finder artifacts already sitting in user environments.

That target should not be implemented by merely accepting more assignment
syntax. It needs a small dialect model, because the emitted source structure and
the runtime lookup rules both changed between `v64` and `v70`.

### Dialects to model

| Dialect                                 | Released range              | Emitted data shape                                   | Lookup behavior that matters                                                                                |
| --------------------------------------- | --------------------------- | ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Early released path-hook finder         | `v64.0.0` through `v68.1.0` | top-level plain `MAPPING = ...` / `NAMESPACES = ...` | recursive nested-lookup behavior predating the immediate-child-only rule                                    |
| Immediate-child plain-assignment finder | `v68.1.1` through `v69.x`   | top-level plain assignments                          | current-style parent/immediate-child `PathFinder` rule, but not the later empty-namespace fallback behavior |
| Current annotated finder                | `v70.0.0+`                  | top-level annotated assignments                      | immediate-child rule plus `NAMESPACES == []` mapping reuse and always-retained placeholder paths            |

The pre-release `v63.0.0b1` class-local finder is real history, but it is not
part of a `v64.0.0+` support promise and should stay out of that scope unless we
make an explicit beta-artifact compatibility decision.

### Parser work

A `v64.0.0+` parser would need to:

1. accept both top-level plain assignments and top-level annotated assignments
    for `MAPPING` / `NAMESPACES`
1. classify the finder dialect from emitted source cues, not from an unavailable
    setuptools version number
1. keep the current literal-only safety posture:
    - dict literals only
    - string module names
    - string mapping paths
    - list-of-string namespace paths
1. reject ambiguous older source shapes rather than “best effort” guessing

Useful dialect cues already identified from history:

- annotated assignments imply the `v70+` dialect
- plain top-level assignments plus the immediate-child
    `fullname.rpartition(".")` / `PathFinder.find_spec(...)` pattern imply the
    `v68.1.1` through `v69.x` dialect
- plain top-level assignments without that immediate-child pattern imply the
    older released `v64.0.0` through `v68.1.0` dialect

### Resolver work

The resolver should branch on the parsed dialect only where setuptools runtime
semantics actually diverge:

1. nested mapping traversal
    - older recursive finder behavior versus the `v68.1.1+`
        immediate-child-only rule
1. empty namespace path lists
    - `v70+` reuses `MAPPING[fullname]`
    - older released forms should not inherit that behavior automatically
1. namespace placeholder retention
    - `v70+` always keeps the placeholder in namespace paths
    - older released forms used a weaker placeholder construction

### Tests to add

The smallest credible `v64.0.0+` expansion would add fixture-based parser and
resolver tests at these release boundaries:

- `v64.0.0`
- `v68.1.1`
- `v70.0.0`

Each fixture should be copied from a real emitted template shape at that boundary
and cover:

- mapping extraction
- namespace extraction
- nested child resolution
- nested descendant behavior
- empty namespace-path behavior where applicable

That test matrix would prevent us from “supporting” an old artifact syntactically
while accidentally resolving it with the wrong generation's runtime semantics.

## Suggested watchpoints

If we revisit this later, these setuptools source anchors are the highest-signal
places to inspect:

- `_TopLevelFinder.template_vars`
- `_TopLevelFinder.get_implementation`
- `_FINDER_TEMPLATE`
- `_EditableFinder.find_spec`
- `_EditableNamespaceFinder._paths`
- `_encode_pth`

And these literals are the best cheap history probes:

```sh
git log --follow -S 'import {finder}; {finder}.install()' -- setuptools/command/editable_wheel.py
git log --follow -S 'MAPPING: dict[str, str] = {mapping!r}' -- setuptools/command/editable_wheel.py
git log --follow -S 'NAMESPACES: dict[str, list[str]] = {namespaces!r}' -- setuptools/command/editable_wheel.py
git log --follow -S 'if not paths and fullname in MAPPING:' -- setuptools/command/editable_wheel.py
```

These probes separate:

- wire-format stability
- parser compatibility boundaries
- resolver-semantics changes
