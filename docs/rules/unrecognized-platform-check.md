# unrecognized-platform-check (PYI007)

Derived from the **flake8-pyi** linter.

## What it does
Check for unrecognized `sys.platform` checks. Platform checks should be
simple string comparisons.

> **Note**
>
> This rule only supports the stub file.

## Why is this bad?
Some checks are too complex for type checkers to understand. Please use
simple string comparisons. Such as `sys.platform == "linux"`.

## Example
Use a simple string comparison instead. Such as `==` or `!=`.
```python
if sys.platform == 'win32':
    # Windows specific definitions
else:
    # Posix specific definitions
```