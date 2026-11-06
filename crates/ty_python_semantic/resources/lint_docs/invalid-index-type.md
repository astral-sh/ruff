## What it does

Checks whether the type of an index or key is valid when reading, assigning, or deleting an item
with subscript syntax. `TypedDict` fields have separate checks for allowed keys and mutations.
Explicit method calls such as `items.__getitem__(key)` use the usual call diagnostics.

## Why is this bad?

Indexing a list with a string raises a `TypeError`. A dictionary accepts keys of different types at
runtime, but inserting an integer key into a `dict[str, int]` breaks the expectation that iterating
over the dictionary yields strings.

## Examples

```python
names = ["Ada", "Grace"]
names["0"]  # error: [invalid-index-type]
names[0]  # OK

scores: dict[str, int] = {}
scores[0] = 10  # error: [invalid-index-type]
scores["Ada"] = 10  # OK
```

## Configuration and suppressions

Before 0.0.97, ty reported these errors as `invalid-argument-type` for reads and deletions, and as
`invalid-assignment` for assignments. To keep a previous severity for invalid indices, set
`invalid-index-type` to that level. It applies to all three operations. Keep the old rules' settings
for any other errors you still want them to control. In a rule-specific `ty: ignore` comment that
suppresses an invalid index, replace the old rule name with `invalid-index-type`.
