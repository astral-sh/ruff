# Plan: Fix TypeVar Identity Issue with Materialization

## Problem

When a `Top[C[T: (int, str)]]` type is used, method calls fail because the `Self` typevar in the method signature has a Bottom-materialized upper bound, while the `Self` in the class's inferable typevars set has no materialization. These are different `TypeVarInstance` objects in Salsa, causing `is_inferable()` to return false and the assignability check to fail.

## Root Cause

Two `TypeVarInstance` objects representing the same logical typevar (e.g., `Self` for a class) are treated as distinct when they differ only in their bounds/constraints due to materialization. This happens because:

1. `TypeVarInstance` is salsa-interned and includes all fields (name, definition, bounds, variance, etc.)
1. When bounds are materialized, a new `TypeVarInstance` is created
1. Checks for "same typevar" use full equality, which includes the materialized bounds
1. The inferable typevars set uses `BoundTypeVarInstance` as keys, which includes the full `TypeVarInstance`

## Solution

Introduce a new concept of "typevar identity" that is separate from the full typevar instance. Two typevars have the same identity if they represent the same logical typevar, regardless of how their bounds have been materialized.

## Implementation Steps

### 1. Create `TypeVarIdentity` (salsa-interned)

**Location**: `crates/ty_python_semantic/src/types.rs`

**Fields** (moved from `TypeVarInstance`):

- `name: Name<'db>` - The typevar's name
- `definition: Option<Definition<'db>>` - Where the typevar was defined
- `kind: TypeVarKind` - Whether it's PEP 695, Legacy, or TypingSelf

**Traits to implement**:

- `Debug` (via salsa)
- `Clone, Copy` (via salsa)
- `PartialEq, Eq` (via salsa)
- `Hash` (via salsa)
- `get_size2::GetSize` (via salsa)

**Salsa attributes**:

```rust
#[salsa::interned(debug)]
pub struct TypeVarIdentity<'db> {
    pub(crate) name: Name<'db>,
    pub(crate) definition: Option<Definition<'db>>,
    pub(crate) kind: TypeVarKind,
}
```

### 2. Create `BoundTypeVarIdentity` (non-interned)

**Location**: `crates/ty_python_semantic/src/types.rs`

**Fields**:

- `identity: TypeVarIdentity<'db>` - The typevar's identity
- `binding_context: BindingContext<'db>` - Where the typevar is bound

**Traits to implement**:

- `Debug`
- `Clone, Copy`
- `PartialEq, Eq`
- `Hash`
- `get_size2::GetSize`

This type identifies a specific binding of a typevar (e.g., `T@ClassC1` vs `T@FunctionF`).

### 3. Update `TypeVarInstance`

**Changes**:

- Add new field: `identity: TypeVarIdentity<'db>`
- Keep existing fields: `_bound_or_constraints`, `explicit_variance`, `_default`, `original`
- Remove fields moved to `TypeVarIdentity`: `name`, `definition`, `kind`

**Constructor updates**:

- Create `TypeVarIdentity` first, then use it in `TypeVarInstance::new`
- Update all call sites that construct `TypeVarInstance`

**Accessor methods**:

- Add forwarding methods for `name()`, `definition()`, `kind()` that delegate to `identity()`
- Keep existing methods for other fields

### 4. Add `identity()` method to `BoundTypeVarInstance`

**Method signature**:

```rust
pub(crate) fn identity(self, db: &'db dyn Db) -> BoundTypeVarIdentity<'db> {
    BoundTypeVarIdentity {
        identity: self.typevar(db).identity(db),
        binding_context: self.binding_context(db),
    }
}
```

### 5. Update `GenericContext`

**Location**: `crates/ty_python_semantic/src/types/generics.rs`

**Changes**:

- Change `variables_inner` from `FxOrderMap<BoundTypeVarInstance<'db>, ...>` to `FxOrderMap<BoundTypeVarIdentity<'db>, ...>`
- Update `variables()` method to return `BoundTypeVarInstance` by looking up the full instance
- Update all methods that use `variables_inner` as a map key

**Affected methods**:

- `from_typevar_instances()` - use `btv.identity(db)` as map key
- `variables()` - reconstruct `BoundTypeVarInstance` from stored identity
- `lookup()` - use identity for lookup

### 6. Update `Specialization`

**Location**: `crates/ty_python_semantic/src/types/generics.rs`

**Changes**:

- The `generic_context` field already uses `GenericContext`, which will now use identities
- Verify that `types` array stays synchronized with context (might need to store parallel arrays or restructure)
- Update methods that iterate over typevars and types together

### 7. Update `ConstraintSet`

**Location**: `crates/ty_python_semantic/src/types.rs` (if it uses typevar keys)

**Changes**:

- If `ConstraintSet` uses `BoundTypeVarInstance` as keys in any internal maps, update to use `BoundTypeVarIdentity`
- Search for uses of `BoundTypeVarInstance` as map keys or set elements

### 8. Update `InferableTypeVars`

**Location**: `crates/ty_python_semantic/src/types/generics.rs`

**Changes**:

- Change from `FxHashSet<BoundTypeVarInstance<'db>>` to `FxHashSet<BoundTypeVarIdentity<'db>>`
- Update `is_inferable()` to use `bound_typevar.identity(db)` for lookup
- Update `inferable_typevars_innerer()` to collect identities instead of full instances

### 9. Remove `is_identical_to` methods

**Location**: `crates/ty_python_semantic/src/types.rs`

**Changes**:

- Remove `TypeVarInstance::is_identical_to()` method entirely
- Remove `BoundTypeVarInstance::is_identical_to()` method entirely
- Update all call sites to use direct equality comparison instead:
    - `btv1.identity(db) == btv2.identity(db)` for bound typevars
    - `tv1.identity(db) == tv2.identity(db)` for typevar instances
- Search for all uses of `is_identical_to` and replace with identity comparisons

**Rationale**: With explicit identity types, we can use standard `==` comparison instead of custom methods. The identity types already implement `Eq` and `PartialEq` correctly.

### 10. Update Display implementations

**Location**: `crates/ty_python_semantic/src/types/display.rs`

**Changes**:

- Update `DisplayBoundTypeVarInstance` to use `typevar.identity(db).name(db)`
- Verify all display code still works correctly

## Testing Strategy

### Primary Testing: Existing Test Suite

Since this branch is based on `main` (not `work`), the regression we identified doesn't exist yet in this branch. Our primary testing goal is to ensure all existing tests continue to pass after the refactoring.

**Process**:

1. Run the full test suite in the new branch after each major step
1. Ensure no regressions are introduced by the refactoring
1. Fix any test failures that arise from the structural changes

### Testing the Regression Fix

To verify that this change fixes the regression that would be introduced by the `work` branch changes:

**Process**:

1. In the `work` worktree, temporarily merge the `dcreager/typevar-identity` branch:

    ```bash
    cd /home/dcreager/git/ruff/work
    git merge dcreager/typevar-identity
    ```

1. Build and run the test case from `/home/dcreager/Documents/scratch/ty/top.py`:

    ```bash
    cargo build --bin ty
    target/debug/ty check /home/dcreager/Documents/scratch/ty/top.py
    ```

1. Verify that the `invalid-argument-type` error no longer occurs for `x.method()`

1. Revert the merge to restore the `work` branch:

    ```bash
    git merge --abort  # or git reset --hard HEAD if merge was completed
    ```

### Why This Approach?

- The `main` branch doesn't have the inferable typevar changes yet, so the bug doesn't manifest
- The `work` branch has the inferable changes that trigger the bug
- By merging our fix into `work`, we can test that it resolves the issue
- We revert to keep the `work` and new feature branches independent

## Migration Notes

- This is a significant refactoring that touches core type system code
- All places that construct `TypeVarInstance` must be updated
- All places that use `BoundTypeVarInstance` for identity/lookup must use `BoundTypeVarIdentity`
- Salsa will need to recompute caches after these changes
- Performance should be similar or slightly better (smaller identity keys for lookups)

## Rollout

1. Implement changes incrementally, ensuring tests pass at each step
1. Start with creating new types (`TypeVarIdentity`, `BoundTypeVarIdentity`)
1. Update `TypeVarInstance` structure
1. Update all construction sites
1. Update data structures (`GenericContext`, `InferableTypeVars`)
1. Update comparison logic
1. Run full test suite
1. Test with real-world cases including the motivating example
