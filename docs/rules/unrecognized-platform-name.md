# unrecognized-platform-name (PYI008)

Derived from the **flake8-pyi** linter.

## What it does
Check for unrecognized platform names in `sys.platform` checks.

**Note**: this rule is only enabled in `.pyi` stub files.

## Why is this bad?
If a `sys.platform` check compares to a platform name outside of a
small set of known platforms (e.g. "linux", "win32", etc.), it's likely
a typo or a platform name that is not recognized by type checkers.

The list of known platforms is: "linux", "win32", "cygwin", "darwin".

## Example
```python
if sys.platform == "linus":
    ...
```

Use instead:
```python
if sys.platform == "linux":
   ...
```

## References
- [PEP 484](https://peps.python.org/pep-0484/#version-and-platform-checking)