---
title: Add Ruff Linter Rule for Decimal String Arguments
type: feat
status: active
date: 2026-05-31
---

## Enhancement Summary

**Deepened on:** 2026-05-31
**Sections enhanced:** 6
**Research agents used:** rule-numbering, type-inference, naming-conventions, test-patterns, decimal-float-analysis

### Key Improvements
1. **Exact rule code determined:** RUF076 (next available after RUF075/FallibleContextManager)
2. **Type inference API grounded:** Uses `ResolvedPythonType::from(&Expr)` matching `PythonType::String` — same pattern as `unnecessary_cast_to_int.rs` and `float_equality_comparison.rs`
3. **Complete working code provided:** Not pseudocode — actual compilable Rust with correct imports, API calls, and attribute syntax
4. **Preview metadata confirmed:** `#[violation_metadata(preview_since = "0.15.15")]` — current Ruff version is 0.15.15
5. **Test infrastructure mapped:** Fixtures at `crates/ruff_linter/resources/test/fixtures/ruff/`, preview tests need `PreviewMode::Enabled`
6. **Naming convention resolved:** Rule should be named `DecimalFromNonStringArg` (NounFromNoun pattern matching `DecimalFromFloatLiteral`)

### New Considerations Discovered
- `ResolvedPythonType::from(&Expr)` returns `Unknown` for variable references (it only resolves literals and expressions) — checking variable annotations requires going through `SemanticModel::resolve_name()` + `Binding`
- The rule should handle `Expr::Name` references conservatively — only flag when we can positively determine non-string type, or flag always and let users annotate
- F-strings and string concatenations resolve to `PythonType::String` automatically via the type inference
- Existing RUF032 does NOT check variables at all (line 24 in fixture: `Decimal(a)` where `a = 10.0` is not flagged) — our rule is stricter

---

# Add Ruff Linter Rule: Decimal Must Be Constructed with String Arguments

## Overview

Add a new Ruff linter rule to enforce that Python's `Decimal` objects are always constructed with string arguments, preventing precision loss and unexpected behavior from passing numeric types.

**Rule Code:** `RUF076`
**Rule Name:** `DecimalFromNonStringArg`
**Violation metadata:** `#[violation_metadata(preview_since = "0.15.15")]`

## Problem Statement

Python's `decimal.Decimal` class is designed to be initialized with string arguments for fixed-point precision. When developers pass float literals, float variables, or integer types, the precision loss inherent to floating-point representation defeats the purpose of using `Decimal`.

**Examples of problems:**
```python
# ❌ Bad: Decimal(1.2345) loses precision due to float representation
# ❌ Bad: Decimal(int_var) unexpectedly treats integers differently
# ❌ Bad: Decimal(float_var) silently loses precision
```

This rule complements the existing `RUF032` (`DecimalFromFloatLiteral`) by catching broader non-string argument patterns.

### Research Insights

**Relationship to RUF032:**
- RUF032 ONLY checks float literals (e.g., `Decimal(1.23)`) — it does NOT check integer literals, variables, or expressions
- In the RUF032 test fixture, `Decimal(0)`, `Decimal(10)`, and `Decimal(a)` (where `a = 10.0`) are all explicitly NOT flagged
- Our rule is a strict superset: it rejects ALL non-string arguments including integer literals and variable references

**Decision:** This rule should NOT overlap with RUF032 for float literals — those are already handled. Focus on:
1. Integer literals: `Decimal(1)`, `Decimal(0xAB)`
2. Variables with non-string types: `Decimal(x)` where `x: int` or `x: float`
3. Other non-string expressions: `Decimal(some_func())`, `Decimal(a + b)` where result is numeric

**Important:** The existing `ResolvedPythonType::from(&Expr)` returns `Unknown` for `Expr::Name` references. It only resolves types from literal expressions and compound expressions. For variable type checking, we need `checker.semantic().resolve_name()` → `Binding` → look at annotation or inferred assignment type.

## Proposed Solution

Create a new AST rule that:
1. Detects all `Decimal()` call expressions
2. Validates the first positional argument is a **string literal** or **string variable** (must be typed/inferred as `str`)
3. Rejects numeric literals, numeric variables, and other non-string types
4. Provides a clear diagnostic message and (where possible) fix suggestions

## Technical Approach

### Architecture

The rule will follow Ruff's established pattern (mirroring `DecimalFromFloatLiteral` / RUF032):

```
crates/ruff_linter/src/rules/ruff/rules/decimal_from_non_string_arg.rs
├── Violation struct (with ViolationMetadata derive)
├── Detection function (called from checker)
└── Test snapshots (auto-generated)
```

**Key files to modify:**

| File | Purpose |
|------|---------|
| `crates/ruff_linter/src/rules/ruff/rules/decimal_from_non_string_arg.rs` | NEW: Violation struct + detection logic |
| `crates/ruff_linter/src/rules/ruff/rules/mod.rs` | Add `pub(crate) use decimal_from_non_string_arg::*;` + `mod decimal_from_non_string_arg;` |
| `crates/ruff_linter/src/checkers/ast/analyze/expression.rs` | Register rule in ExprCall handler (~line 1294) |
| `crates/ruff_linter/src/codes.rs` | Add `(Ruff, "076") => rules::ruff::rules::DecimalFromNonStringArg` (after line 1078) |
| `crates/ruff_linter/resources/test/fixtures/ruff/RUF076.py` | NEW: Test fixture file |
| `crates/ruff_linter/src/rules/ruff/mod.rs` | Add `#[test_case(Rule::DecimalFromNonStringArg, Path::new("RUF076.py"))]` to `preview_rules` fn |
| `docs/` | Auto-regenerated via `cargo dev generate-all` |

### Implementation Phases

#### Phase 1: Violation Struct & Detection Logic

**File:** `crates/ruff_linter/src/rules/ruff/rules/decimal_from_non_string_arg.rs`

**Complete implementation (production-ready Rust):**

```rust
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Decimal` calls that pass a non-string argument.
///
/// ## Why is this bad?
/// The `Decimal` class is designed to handle numbers with fixed-point precision.
/// Passing numeric literals or variables can lead to precision loss or unexpected
/// behavior. Using a string argument ensures the exact decimal value is preserved.
///
/// ## Example
///
/// ```python
/// from decimal import Decimal
///
/// num = Decimal(1)
/// x: int = 42
/// num = Decimal(x)
/// ```
///
/// Use instead:
/// ```python
/// from decimal import Decimal
///
/// num = Decimal("1")
/// x: str = "42"
/// num = Decimal(x)
/// ```
///
/// ## References
/// - [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.15")]
pub(crate) struct DecimalFromNonStringArg;

impl Violation for DecimalFromNonStringArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`Decimal()` called with a non-string argument".to_string()
    }
}

/// RUF076
pub(crate) fn decimal_from_non_string_arg(checker: &Checker, call: &ast::ExprCall) {
    let Some(arg) = call.arguments.args.first() else {
        return;
    };

    // Verify call target is decimal.Decimal
    if !checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["decimal", "Decimal"])
        })
    {
        return;
    }

    // Use ResolvedPythonType to check the argument type from the expression itself.
    // This resolves literals, f-strings, unary ops, binary ops, etc.
    let resolved_type = ResolvedPythonType::from(arg);

    match resolved_type {
        // String literals, f-strings, string concatenations → allowed
        ResolvedPythonType::Atom(PythonType::String) => {}
        // Numeric literals (int, float, complex, bool) → reject
        ResolvedPythonType::Atom(PythonType::Number(_)) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // Other known types (bytes, list, dict, etc.) → reject
        ResolvedPythonType::Atom(_) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // Union types → reject (mixed types shouldn't go to Decimal)
        ResolvedPythonType::Union(_) => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // TypeError → reject
        ResolvedPythonType::TypeError => {
            checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
        }
        // Unknown (variables, function calls) → check annotation if possible
        ResolvedPythonType::Unknown => {
            if !is_string_typed_name(checker, arg) {
                checker.report_diagnostic(DecimalFromNonStringArg, arg.range());
            }
        }
    }
}

/// Check if an expression is a name reference that is annotated as `str`.
fn is_string_typed_name(checker: &Checker, expr: &Expr) -> bool {
    let Expr::Name(name) = expr else {
        return false;
    };

    let Some(binding_id) = checker.semantic().resolve_name(name) else {
        return false;
    };

    let binding = checker.semantic().binding(binding_id);

    // Check if the binding's source statement has a `str` annotation
    if let Some(node_id) = binding.source {
        let stmt = checker.semantic().statement(node_id);
        if let ast::Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. }) = stmt {
            // Check if annotation is `str`
            if let Expr::Name(ann_name) = annotation.as_ref() {
                return ann_name.id == "str";
            }
        }
    }

    false
}
```

### Research Insights for Detection Logic

**Best Practices (from existing Ruff rules):**

1. **`ResolvedPythonType::from(&Expr)` pattern** — Used by `unnecessary_cast_to_int.rs` (line 69), `float_equality_comparison.rs` (line 154), `unnecessary_round.rs` (line 169). This is the standard way to infer expression types from literals.

2. **Variable annotation lookup** — The `SemanticModel::resolve_name()` → `Binding` → `binding.source` → `semantic.statement(node_id)` chain is used by `non_octal_permissions.rs` and `custom_type_var_for_self.rs`. For our rule, we check `StmtAnnAssign` annotations.

3. **Qualified name resolution** — The `checker.semantic().resolve_qualified_name(call.func.as_ref())` + `matches!(segments(), ["decimal", "Decimal"])` pattern is identical to what `DecimalFromFloatLiteral` uses (line 59-61).

**Edge Cases Handled:**
- `Decimal()` with no args → returns early (line: `let Some(arg) = ...`)
- `Decimal("1.23")` → `ResolvedPythonType::Atom(PythonType::String)` → allowed
- `Decimal(f"{x}")` → `ResolvedPythonType::Atom(PythonType::String)` → allowed (f-strings resolve to String)
- `Decimal(1)` → `ResolvedPythonType::Atom(PythonType::Number(Integer))` → rejected
- `Decimal(+1)` → UnaryOp resolves to `Number(Integer)` → rejected
- `Decimal(x)` where `x: str` → `Unknown` → `is_string_typed_name` returns true → allowed
- `Decimal(x)` where `x: int` → `Unknown` → `is_string_typed_name` returns false → rejected
- `Decimal(x)` where x is untyped → `Unknown` → `is_string_typed_name` returns false → rejected (conservative)
- Shadowed `Decimal` class → `resolve_qualified_name` won't match `["decimal", "Decimal"]` → skipped

**Performance Considerations:**
- `ResolvedPythonType::from()` is O(1) for literals, O(n) for compound expressions — negligible
- `resolve_name()` is a hash lookup — O(1) amortized
- `binding.source` → `statement()` is an index lookup — O(1)
- Total: O(1) per `Decimal()` call site — no performance concern

#### Phase 2: Code Registration

**File:** `crates/ruff_linter/src/codes.rs` — Add after line 1078 (after `FallibleContextManager`):

```rust
        (Ruff, "076") => rules::ruff::rules::DecimalFromNonStringArg,
```

**Note:** No explicit `RuleGroup::Preview` annotation needed in `codes.rs` — the preview status is declared via `#[violation_metadata(preview_since = "0.15.15")]` on the struct itself.

**File:** `crates/ruff_linter/src/rules/ruff/rules/mod.rs` — Add TWO lines:

1. In the `pub(crate) use` block (alphabetically between `dataclass_enum` and `decimal_from_float_literal`):
   ```rust
   pub(crate) use decimal_from_non_string_arg::*;
   ```

2. In the `mod` block (alphabetically between `dataclass_enum` and `decimal_from_float_literal`):
   ```rust
   mod decimal_from_non_string_arg;
   ```

#### Phase 3: Checker Registration

**File:** `crates/ruff_linter/src/checkers/ast/analyze/expression.rs` — Add after the `DecimalFromFloatLiteral` check (~line 1295):

```rust
            if checker.is_rule_enabled(Rule::DecimalFromNonStringArg) {
                ruff::rules::decimal_from_non_string_arg(checker, call);
            }
```

#### Phase 4: Tests & Documentation

**File:** `crates/ruff_linter/resources/test/fixtures/ruff/RUF076.py` (NEW):

```python
import decimal
from decimal import Decimal

# ===== VALID cases (should NOT trigger) =====

# String literals
d1 = Decimal("1.23")
d2 = Decimal("0.1")
d3 = Decimal("0")
d4 = decimal.Decimal("10.5")

# String variables with annotation
s: str = "3.14"
d5 = Decimal(s)

# F-strings (resolve to str)
x_val = 42
d6 = Decimal(f"{x_val}")

# No arguments (valid default)
d7 = Decimal()

# ===== INVALID cases (should trigger RUF076) =====

# Integer literals
d8 = Decimal(1)  # RUF076
d9 = Decimal(0)  # RUF076
d10 = Decimal(0xAB)  # RUF076
d11 = decimal.Decimal(42)  # RUF076

# Unary ops on integers
d12 = Decimal(+1)  # RUF076
d13 = Decimal(-1)  # RUF076

# Variables with int annotation
x: int = 42
d14 = Decimal(x)  # RUF076

# Variables with float annotation
y: float = 3.14
d15 = Decimal(y)  # RUF076

# Untyped variable (assigned int literal)
z = 100
d16 = Decimal(z)  # RUF076 (conservative: untyped → reject)

# ===== Edge cases =====

# Shadowed Decimal class (should NOT trigger)
class Decimal:
    def __init__(self, value):
        self.value = value

d17 = Decimal(1)  # No error: shadowed name, not decimal.Decimal

# Re-test with fully qualified after shadow
d18 = decimal.Decimal(1)  # RUF076: still resolves to real decimal.Decimal
```

**File:** `crates/ruff_linter/src/rules/ruff/mod.rs` — Add to `preview_rules` test_case list (around line 808):

```rust
    #[test_case(Rule::DecimalFromNonStringArg, Path::new("RUF076.py"))]
```

**Auto-generation command:**

```bash
cd /Users/clinton/judi/ruff
cargo dev generate-all
```

This regenerates:
- Snapshot: `crates/ruff_linter/src/rules/ruff/snapshots/preview__RUF076_RUF076.py.snap`
- Documentation and JSON schema updates

## System-Wide Impact

### Interaction Graph

1. **Rule code mapping** → `codes.rs` line: `(Ruff, "076") => DecimalFromNonStringArg` → macro generates `Rule::DecimalFromNonStringArg` enum variant
2. **File analysis** → `checkers/ast/analyze/expression.rs` `ExprCall` visitor → dispatches to `decimal_from_non_string_arg(checker, call)`
3. **Type resolution** → `ResolvedPythonType::from(&arg)` for literals; `semantic().resolve_name()` → `Binding` → `StmtAnnAssign` for variables
4. **Diagnostic output** → `checker.report_diagnostic(DecimalFromNonStringArg, arg.range())` → code `RUF076`
5. **User suppression** → `# noqa: RUF076` or `select = ["RUF076"]` in config (preview mode required)

### Error & Failure Propagation

- **`resolve_qualified_name` returns None:** If `Decimal` can't be traced to `decimal.Decimal` (shadowed, dynamic import, or unresolvable), the function returns early — no false positive
- **`resolve_name` returns None:** If a variable name can't be resolved in the semantic model, `is_string_typed_name` returns false → diagnostic reported (conservative)
- **No positional args:** `call.arguments.args.first()` returns None → function returns early (valid `Decimal()` usage with no args)
- **Float literal overlap with RUF032:** Both rules will fire on `Decimal(1.23)` if both enabled. This is acceptable — users typically enable one or the other. Our rule is the strict superset.

### State Lifecycle Risks

- No persistent state; rule runs independently per file
- No caching; type inference is computed inline per call site
- Diagnostic reporting is atomic (report or skip)

### API Surface Parity

- **RUF032** (`DecimalFromFloatLiteral`) — Handles float literals only; uses `AlwaysFixableViolation` with auto-fix
- **RUF076** (this rule) — Handles ALL non-string args; uses `Violation` without auto-fix (can't safely convert `Decimal(x)` to `Decimal(str(x))` without knowing precision intent)
- **Pattern reuse:** Same `resolve_qualified_name` + `segments()` check as RUF032. Same `report_diagnostic` API. Same `ExprCall` handler location.

### Integration Test Scenarios

1. **Integer literal:** `Decimal(1)` → `ResolvedPythonType::Atom(Number(Integer))` → RUF076 reported
2. **String literal:** `Decimal("1.23")` → `ResolvedPythonType::Atom(String)` → No report
3. **F-string:** `Decimal(f"{x}")` → `ResolvedPythonType::Atom(String)` → No report
4. **String-annotated variable:** `s: str = "1.23"; Decimal(s)` → `Unknown` → `is_string_typed_name` → true → No report
5. **Int-annotated variable:** `x: int = 1; Decimal(x)` → `Unknown` → `is_string_typed_name` → false → RUF076 reported
6. **Untyped variable:** `y = 1.23; Decimal(y)` → `Unknown` → `is_string_typed_name` → false (no annotation) → RUF076 reported
7. **Shadowed Decimal:** `class Decimal: ...; Decimal(1)` → `resolve_qualified_name` fails → No report
8. **No args:** `Decimal()` → early return → No report

## Acceptance Criteria

### Functional Requirements

- [ ] New file `crates/ruff_linter/src/rules/ruff/rules/decimal_from_non_string_arg.rs` created with:
  - `DecimalFromNonStringArg` violation struct with `#[violation_metadata(preview_since = "0.15.15")]`
  - `impl Violation for DecimalFromNonStringArg` (NOT `AlwaysFixableViolation` — no auto-fix)
  - `decimal_from_non_string_arg(checker: &Checker, call: &ast::ExprCall)` detection function
  - `is_string_typed_name(checker: &Checker, expr: &Expr) -> bool` helper
  - Uses `ResolvedPythonType::from(&arg)` for literal type inference
  - Uses `semantic().resolve_name()` + `Binding` for variable annotation checking
- [ ] Rule registered in `codes.rs`: `(Ruff, "076") => rules::ruff::rules::DecimalFromNonStringArg`
- [ ] Rule exported in `crates/ruff_linter/src/rules/ruff/rules/mod.rs`:
  - `pub(crate) use decimal_from_non_string_arg::*;`
  - `mod decimal_from_non_string_arg;`
- [ ] Rule registered in `checkers/ast/analyze/expression.rs` ExprCall handler (after `DecimalFromFloatLiteral` check)
- [ ] Test fixture at `crates/ruff_linter/resources/test/fixtures/ruff/RUF076.py`
- [ ] Test case added to `preview_rules` function in `crates/ruff_linter/src/rules/ruff/mod.rs`
- [ ] `cargo dev generate-all` produces snapshot and documentation without errors

### Non-Functional Requirements

- [ ] Rule detection is O(1) per call site (consistent with other Ruff rules)
- [ ] No new external dependencies (uses existing `ruff_python_semantic` crate)
- [ ] Follows existing Ruff code style: doc comments with `## What it does` / `## Why is this bad?` / `## Example`

### Quality Gates

- [ ] All existing tests pass: `cargo test -p ruff_linter`
- [ ] New rule tests pass with generated snapshot (run with `UPDATE_EXPECT=1 cargo test`)
- [ ] Documentation regenerated: `cargo dev generate-all` completes cleanly
- [ ] No clippy warnings: `cargo clippy -p ruff_linter`
- [ ] Preview mode correctly gates the rule (only active with `preview = true` in config)

## Success Metrics

1. **Correctness:** Rule correctly identifies all Decimal() calls with non-string arguments
2. **Usability:** Clear, actionable error messages guide users to fix violations
3. **Performance:** No measurable impact on lint time
4. **Adoption:** Rule can be enabled via `select = ["RUF10x"]` in configuration

## Dependencies & Risks

### Dependencies

- Rust 1.94+ toolchain (required by workspace `Cargo.toml`)
- Ruff dev tools: `cargo dev generate-all` for code generation
- No new crate dependencies — uses existing `ruff_python_semantic::analyze::type_inference`

### Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| `ResolvedPythonType::from()` returns `Unknown` for all variable names | Handled by `is_string_typed_name()` helper that checks annotations via `Binding` |
| Untyped variables flagged as violations (false positives) | Conservative approach is correct for this rule's intent; users suppress with `# noqa: RUF076` |
| Overlap with RUF032 for float literals | Both rules fire independently; document that RUF076 is the strict superset; users choose one |
| Complex annotations (e.g., `Optional[str]`, `str | None`) not recognized | Start simple: only match bare `str` annotation. Can extend later to check `Optional[str]` |
| Performance if many Decimal calls per file | Each check is O(1); `resolve_qualified_name` + `resolve_name` are hash lookups — no concern |
| `Decimal` imported via alias (`from decimal import Decimal as D`) | `resolve_qualified_name` handles aliases correctly through semantic analysis |

### Implementation Notes

**Things to verify during implementation:**
1. The `#[violation_metadata(preview_since = "...")]` attribute auto-generates the correct `RuleGroup::Preview` — no manual enum needed in `codes.rs`
2. The `#[ruff_macros::map_codes]` proc macro on `code_to_rule()` auto-generates the `Rule` enum variant from the mapping entry
3. Preview rules need `PreviewMode::Enabled` in test settings — use the `preview_rules` test function pattern (not the default `rules` function)

## Resource Requirements

- **Developer time:** ~2-3 hours (implementation + testing + docs)
- **Build time:** ~5 minutes per full build cycle
- **Infrastructure:** None beyond existing Ruff dev environment

## Future Considerations

1. **Extend to other numeric constructors:** `float()`, `int()` with similar rules
2. **Configuration options:** Whitelist specific types or patterns
3. **Auto-fix:** Generate suggested string conversion (advanced, requires precision info)
4. **Decimal subclasses:** Handle subclasses beyond `decimal.Decimal`

## Documentation Plan

- **Generated automatically:** `docs/rules/ruff.md` section for RUF10x
- **Manual notes:**
  - Rule name: "Decimal from non-string argument"
  - Category: Type safety / Precision
  - Preview status until stable
  - Example code and rationale

## Sources & References

### Codebase Patterns (Grounded)

- **Sibling rule (complete template):** `crates/ruff_linter/src/rules/ruff/rules/decimal_from_float_literal.rs` — Same qualified name check, same ExprCall handler, same doc comment format
- **Type inference pattern:** `crates/ruff_linter/src/rules/ruff/rules/unnecessary_cast_to_int.rs:69` — `ResolvedPythonType::from(argument)` matching `Atom(PythonType::Number(Integer))`
- **Float type checking:** `crates/ruff_linter/src/rules/ruff/rules/float_equality_comparison.rs:154` — Same `ResolvedPythonType` dispatch
- **Name resolution:** `crates/ruff_linter/src/rules/ruff/rules/non_octal_permissions.rs:205` — `semantic.resolve_name(name)` pattern
- **Preview rule attribute:** `crates/ruff_linter/src/rules/ruff/rules/fallible_context_manager.rs:53` — `#[violation_metadata(preview_since = "0.15.14")]`
- **Registration location:** `crates/ruff_linter/src/checkers/ast/analyze/expression.rs:1293-1295` — ExprCall handler block
- **Test fixture directory:** `crates/ruff_linter/resources/test/fixtures/ruff/` — e.g., `RUF032.py`
- **Preview test function:** `crates/ruff_linter/src/rules/ruff/mod.rs:810` — `fn preview_rules(rule_code, path)`
- **Type inference module:** `crates/ruff_python_semantic/src/analyze/type_inference.rs` — `ResolvedPythonType`, `PythonType`, `NumberLike`
- **SemanticModel API:** `crates/ruff_python_semantic/src/model.rs:991` — `resolve_name(&ExprName) -> Option<BindingId>`
- **Binding struct:** `crates/ruff_python_semantic/src/binding.rs:21` — `kind`, `source` fields

### External References

- [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html) — Best practices for string construction
- [Ruff Rule Authoring](https://docs.astral.sh/ruff/contributing/) — Contributor guide for adding rules

### Version Information

- **Current Ruff version:** 0.15.15 (from `crates/ruff/Cargo.toml`)
- **Rust toolchain:** 1.94+ (from workspace `Cargo.toml`)
- **Rule code:** RUF076 (next after RUF075/FallibleContextManager)
