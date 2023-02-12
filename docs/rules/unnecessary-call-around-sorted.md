# unnecessary-call-around-sorted (C413)

Derived from the **flake8-comprehensions** linter.

Autofix is always available.

## What it does
Checks for unnecessary `list` or `reversed` calls around the `sorted` functions.

## Why is this bad?
It is unnecessary to use `list()` around `sorted()` as it already returns a list.
It is also unnecessary to use `reversed()` around `sorted()` as the latter has a reverse argument.

## Examples
```python
reversed(sorted(iterable))
```

Use instead:
```python
sorted(iterable, reverse=True)
```