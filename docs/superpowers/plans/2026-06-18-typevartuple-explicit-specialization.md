# TypeVarTuple Explicit Specialization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recognize and validate TypeVarTuple and Unpack, support precise explicit class and alias specialization, and conservatively use a gradual tuple whenever a pack would require inference.

**Architecture:** Represent a TypeVarTuple as one generic-context slot whose specialization value is a tuple. Explicit subscripting constructs that tuple and ordinary type mapping splices it into variable-length tuple structure. Call inference excludes TypeVarTuple identities and finalizes unresolved packs as `tuple[Unknown, ...]`.

**Tech Stack:** Rust, Salsa, Ruff mdtests, cargo-nextest, insta snapshots.

______________________________________________________________________

## Task 1: Recognize and validate TypeVarTuple and Unpack

**Files:**

- Create: `crates/ty_python_semantic/resources/mdtest/generics/legacy/typevartuple.md`

- Create: `crates/ty_python_semantic/resources/mdtest/generics/legacy/unpack.md`

- Create: `crates/ty_python_semantic/resources/mdtest/generics/pep695/typevartuple.md`

- Modify: `crates/ty_python_semantic/src/types/typevar.rs`

- Modify: `crates/ty_python_semantic/src/types/class/known.rs`

- Modify: `crates/ty_python_semantic/src/types/known_instance.rs`

- Modify: `crates/ty_python_semantic/src/types/infer.rs`

- Modify: `crates/ty_python_semantic/src/types/infer/builder.rs`

- Modify: `crates/ty_python_semantic/src/types/infer/builder/typevar.rs`

- Modify: `crates/ty_python_semantic/src/types/infer/builder/type_expression.rs`

- Modify: `crates/ty_python_semantic/src/types/infer/builder/post_inference/type_param_validation.rs`

- [ ] Add minimal failing mdtests covering a valid legacy declaration, a PEP 695 declaration, `__name__`, invalid bare use, invalid `Unpack[int]`, and multiple packs.

- [ ] Run each new mdtest and confirm failures contain the existing TypeVarTuple `Todo` behavior.

- [ ] Add TypeVarTuple kinds and construction/validation paths, replacing TypeVarTuple-specific dynamic placeholders only where the new representation is available.

- [ ] Run the three focused mdtests and inspect all generated snapshots or pending snapshots.

- [ ] Commit as `[ty] Recognize TypeVarTuple and Unpack type forms`.

The initial assertions include:

```py
Ts = TypeVarTuple("Ts")
reveal_type(Ts)  # revealed: TypeVarTuple
reveal_type(Ts.__name__)  # revealed: Literal["Ts"]

def invalid(x: Ts) -> None: ...  # error: [invalid-type-form]
```

## Task 2: Add explicit specialization and tuple substitution

**Files:**

- Modify: `crates/ty_python_semantic/resources/mdtest/generics/legacy/typevartuple.md`

- Modify: `crates/ty_python_semantic/resources/mdtest/generics/legacy/unpack.md`

- Modify: `crates/ty_python_semantic/resources/mdtest/generics/pep695/typevartuple.md`

- Modify: `crates/ty_python_semantic/src/types/generics.rs`

- Modify: `crates/ty_python_semantic/src/types/infer/builder/subscript.rs`

- Modify: `crates/ty_python_semantic/src/types/tuple.rs`

- Modify: `crates/ty_python_semantic/src/types/signatures.rs`

- Modify: `crates/ty_python_semantic/src/types.rs`

- [ ] Add failing tests for empty, fixed, unbounded, prefixed, suffixed, and middle explicit pack specializations.

- [ ] Run the focused mdtests and confirm they reveal nested or gradual placeholder tuple types instead of the expected precise types.

- [ ] Add TypeVarTuple to generic contexts as one slot and partition explicit arguments around it.

- [ ] Represent the mapped pack as one tuple and splice it in `VariableLengthTuple<Type>::apply_type_mapping_impl` while preserving fixed prefix and suffix elements.

- [ ] Apply a declared pack default, otherwise use `tuple[Unknown, ...]` for an omitted pack.

- [ ] Run focused mdtests and the existing generic class and alias mdtests; inspect snapshots.

- [ ] Commit as `[ty] Support explicit TypeVarTuple specialization`.

The required specialization assertions include:

```py
class Between[T, *Ts, U]:
    value: tuple[T, *Ts, U]

reveal_type(Between[int, bool, bytes, str]().value)  # revealed: tuple[int, bool, bytes, str]
reveal_type(Between[int, *tuple[bool, ...], str]().value)  # revealed: tuple[int, *tuple[bool, ...], str]
```

## Task 3: Prevent TypeVarTuple inference

**Files:**

- Modify: `crates/ty_python_semantic/resources/mdtest/generics/legacy/typevartuple.md`

- Modify: `crates/ty_python_semantic/resources/mdtest/generics/legacy/unpack.md`

- Modify: `crates/ty_python_semantic/resources/mdtest/generics/pep695/typevartuple.md`

- Modify: `crates/ty_python_semantic/src/types/generics.rs`

- Modify: `crates/ty_python_semantic/src/types/call/bind.rs` only if the inference boundary cannot be enforced centrally.

- [ ] Add failing tests proving constructor, tuple-argument, `*args`, callable, repeated-pack, and overload calls reveal a gradual pack instead of an inferred heterogeneous pack.

- [ ] Run the focused mdtests and confirm at least one call currently produces a scalar union, nested tuple, or precise inferred pack.

- [ ] Exclude TypeVarTuple identities from solver mappings while retaining inference for ordinary TypeVars in the same context.

- [ ] Finalize every unresolved pack as `tuple[Unknown, ...]`; do not add argument aggregation, callable matching, repeated-pack merging, or length-based overload logic.

- [ ] Run focused mdtests and existing ParamSpec/callable tests to ensure their inference is unchanged.

- [ ] Commit as `[ty] Use gradual fallback for unsolved TypeVarTuple packs`.

The boundary assertion includes:

```py
def collect[*Ts](*args: *Ts) -> tuple[*Ts]: ...
reveal_type(collect(1, "a"))  # revealed: tuple[Unknown, ...]
```

## Task 4: Add IDE display and safe recovery

**Files:**

- Create: `crates/ty_python_semantic/resources/mdtest/regression/typevartuple_exception_handler.md`

- Modify: `crates/ty_python_semantic/src/types/display.rs`

- Modify: `crates/ty_ide/src/hover.rs`

- Modify: `crates/ty_ide/src/goto_type_definition.rs`

- Modify: `crates/ty_python_semantic/src/types/infer/builder.rs`

- [ ] Add failing hover/go-to-definition coverage and exception-handler recovery mdtests.

- [ ] Run focused tests and confirm missing IDE data or the recovery failure.

- [ ] Add TypeVarTuple display/IDE handling and replace exception recovery assumptions with checked conversions.

- [ ] Run the IDE and recovery tests and inspect snapshots.

- [ ] Commit as `[ty] Add TypeVarTuple IDE and recovery support`.

## Task 5: Verify repository, conformance, and ecosystem behavior

**Files:**

- Modify only files required by failures found in this task.

- [ ] Run `cargo nextest run -p ty_python_semantic` with the repository snapshot environment and inspect every changed snapshot and `.pending-snap` file.

- [ ] Run `cargo nextest run -p ty_ide` with the same environment.

- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.

- [ ] Run `uvx prek run --files` with every changed file.

- [ ] Compare typing conformance with the pinned suite; require no regression in previously passing files.

- [ ] Run or obtain the ty ecosystem comparison; reject unexplained inference-driven diagnostics and check `static_frame` performance.

- [ ] Commit any focused verification corrections separately from earlier commits.
