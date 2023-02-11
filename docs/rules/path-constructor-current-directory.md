# path-constructor-current-directory (PTH200)

Derived from the **flake8-use-pathlib** linter.

Autofix is sometimes available.

## What it does
This rule detects pathlib's `Path` initializations with the default current directory argument.

## Why is this bad?
The `Path()` constructor defaults to the current directory, so don't pass the
current directory (`"."`) explicitly.

## Example
```python
from pathlib import Path

_ = Path(".")
```

Use instead:
```python
from pathlib import Path

_ = Path()
```