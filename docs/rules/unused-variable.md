# unused-variable (F841)

Derived from the **Pyflakes** linter.

Autofix is always available.

## What it does
Checks for the presence of unused variables in function scopes.

## Why is this bad?
A variable that is defined but not used is likely a mistake, and should be
removed to avoid confusion.

If a variable is intentionally defined-but-not-used, it should be prefixed
with an underscore, or some other value that adheres to the
[`dummy-variable-rgx`](https://github.com/charliermarsh/ruff#dummy-variable-rgx) pattern.

## Example
```python
def foo():
    x = 1
    y = 2
    return x
```

Use instead:
```python
def foo():
    x = 1
    return x
```