## What it does

Checks for accesses of submodules that might not've been imported.

## Why is this bad?

When module `a` has a submodule `b`, `import a` isn't generally enough to let you access
`a.b.` You either need to explicitly `import a.b`, or else you need the `__init__.py` file
of `a` to include `from . import b`. Without one of those, `a.b` is an `AttributeError`.

## Examples

```python
import html

# AttributeError: module 'html' has no attribute 'parser'
html.parser  # error
```
