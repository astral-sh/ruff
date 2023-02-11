# commented-out-code (ERA001)

Derived from the **eradicate** linter.

Autofix is always available.

## What it does
Checks for commented-out Python code.

## Why is this bad?
Commented-out code is dead code, and is often included inadvertently.
It should be removed.

## Example
```python
# print('foo')
```