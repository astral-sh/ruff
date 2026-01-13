# Root Cause Analysis: ty Issue #2479

## Summary

The `no-matching-overload` error when using `cv2.imread()` result with `np.average()` is caused by an **incompatibility between cv2 and numpy 2.x type stubs**.

## Key Finding: NumPy 2.x Stub Changes

The issue is **version-dependent**:

| NumPy Version | Pyright | ty |
|---------------|---------|-----|
| 1.26.4        | ✅ Pass  | ✅ Pass |
| 2.x           | ❌ Fail  | ❌ Fail |

**Both type checkers behave identically.** The reporter likely tested with numpy 1.x where Pyright passes.

## What Changed in NumPy 2.x

The numpy type stubs changed how `_ArrayLikeFloat_co` is defined:

**NumPy 1.26.4 (inner union):**
```python
_ArrayLikeFloat_co = _DualArrayLike[
    dtype[Union[bool_, integer[Any], floating[Any]]],  # Union INSIDE dtype
    Union[bool, int, float],
]
```

**NumPy 2.x (outer union):**
```python
_ArrayLikeFloat_co: TypeAlias = _DualArrayLike[
    dtype[np.bool] | dtype[integer[Any]] | dtype[floating[Any]],  # Union OUTSIDE dtype
    bool | int | float,
]
```

## Why This Matters

**cv2 stubs define:**
```python
NumPyArrayNumeric = ndarray[Any, dtype[integer[Any] | floating[Any]]]  # Inner union
```

**Compatibility:**
- With numpy 1.26.4: cv2's `dtype[integer | floating]` matches numpy's `dtype[bool | integer | floating]` (both inner unions)
- With numpy 2.x: cv2's `dtype[integer | floating]` does NOT match numpy's `dtype[bool] | dtype[integer] | dtype[floating]` (inner vs outer union)

## Type Theory Explanation

For a covariant generic `C[T]`:
- `C[A | B]` means "a C containing something that is A or B"
- `C[A] | C[B]` means "either a C[A] or a C[B]"

Type checkers (correctly) do not consider these equivalent because:
- `C[A | B] <: C[A] | C[B]` would require `C[A | B]` to be a subtype of `C[A]` OR `C[B]`
- For covariant C, `C[A | B] <: C[A]` requires `A | B <: A`, which is false

## Special Case: `type[]`

ty does implement union distribution for Python's special `type[]` construct:

```rust
// From crates/ty_python_semantic/src/types/subclass_of.rs:82-92
// Handle unions by distributing `type[]` over each element:
// `type[A | B]` -> `type[A] | type[B]`
```

This is why `type[int | str]` becomes `type[int] | type[str]`. However, this special handling is not applied to arbitrary generic classes like `dtype`.

## Conclusion

**This is not a bug in ty.** Both ty and Pyright exhibit the same behavior.

The issue is a **compatibility problem between cv2 stubs and numpy 2.x stubs**:
- numpy 2.x changed from inner union (`dtype[A | B]`) to outer union (`dtype[A] | dtype[B]`)
- cv2 stubs still use the inner union form
- These forms are not type-compatible

## Recommended Fix

The fix should be in the **opencv-python stubs** (upstream), not in ty. The cv2 stubs should update `NumPyArrayNumeric` from:
```python
ndarray[Any, dtype[integer[Any] | floating[Any]]]
```
to:
```python
ndarray[Any, dtype[integer[Any]] | dtype[floating[Any]]]
```

Alternatively, users can pin to numpy <2.0 as a workaround.
