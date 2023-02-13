# deprecated-type-alias (NPY001)

Autofix is always available.

## What it does

Checks for deprecated numpy type aliases.

## Why is this bad?

For a long time, np.int has been an alias of the builtin int.
This is repeatedly a cause of confusion for newcomers, and existed mainly for historic reasons.
These aliases have been deprecated in 1.20, and removed in 1.24.

## Examples

```python
numpy.bool
```

Use instead:

```python
bool
```