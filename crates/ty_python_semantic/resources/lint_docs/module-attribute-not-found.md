## What it does

Checks for names in a `from ... import ...` statement that ty cannot find in the resolved module.
For a package, an importable submodule also satisfies the import.

## Why is this bad?

Importing a missing name from an existing module raises `ImportError` at runtime. Check the spelling
of the imported name and whether the installed version of the module provides it.

## Examples

```python
from math import sine  # error
```

Use instead:

```python
from math import sin
```

## See also

- `import-not-found` covers missing modules and explains how to update existing settings and ignore
    comments after the split.

## Rule history

- v0.0.102: Split from `import-not-found`.
