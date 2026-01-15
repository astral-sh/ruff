## Summary

Fixes #18775

The SIM910 (`dict-get-with-none-default`) and SIM911 (`zip-dict-keys-and-values`) rules were silently deleting comments when applying fixes.

For example, SIM910 would transform:

```python
ages = {"Tom": 23, "Maria": 23, "Dog": 11}
age = ages.get(  # comment
    "Cat", None
)
```

into:

```python
age = ages.get("Cat")
```

And SIM911 would transform:

```python
for k, v in zip(
    d.keys(),  # comment
    d.values()
):
    ...
```

into:

```python
for k, v in d.items():
    ...
```

This fix marks such transformations as **unsafe** when comments would be deleted, requiring users to explicitly opt-in with `--unsafe-fixes`.

## Approach

Uses the established `CommentRanges::intersects()` pattern already used by other rules in the codebase (as suggested by @MichaReiser in the issue). When comments exist within the expression's range, the fix is marked as `Applicability::Unsafe` instead of `Applicability::Safe`.

## Test plan

- Added test cases for comment preservation in both `SIM910.py` and `SIM911.py`
- Verified that fixes with comments are marked as unsafe ("note: This is an unsafe fix and may change runtime behavior")
- Verified that fixes without comments remain safe
- All existing tests pass
- Clippy and prek checks pass
