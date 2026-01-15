## Summary

Fixes #19745

The RUF010 (`explicit-f-string-type-conversion`) rule was silently deleting comments when transforming f-string expressions like:

```python
f"{ascii(
    # comment
    1
)}"
```

into:

```python
f"{1!a}"
```

This fix marks such transformations as **unsafe** when comments would be deleted, requiring users to explicitly opt-in with `--unsafe-fixes`.

## Approach

Uses the established `CommentRanges::intersects()` pattern already used by other rules in the codebase. When comments exist within the call expression's range, the fix is marked as `Applicability::Unsafe` instead of `Applicability::Safe`.

This is a simpler approach than the one proposed in #19772, following the reviewer's suggestion to use existing infrastructure rather than creating custom enums.

## Test plan

- Added test cases for comment preservation in `RUF010.py`
- Verified manually:
  - `f"{ascii(# comment\n1)}"` requires `--unsafe-fixes` to apply
  - `f"{ascii(1)}"` applies without `--unsafe-fixes` (safe fix)
- All existing tests pass
- Clippy and prek checks pass
