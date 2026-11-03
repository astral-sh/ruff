## What it does

Checks for imports where ty cannot find the module or a member named in a `from` import.

## Why is this bad?

Importing a missing module raises `ModuleNotFoundError` at runtime. Importing a missing member from
an existing module raises `ImportError`.

These diagnostics can often be caused due to ty's Python environment or search paths being
configured incorrectly. See the
[import troubleshooting FAQ](https://docs.astral.sh/ty/reference/typing-faq/#why-cant-ty-resolve-my-imports)
for more details.

## Examples

```python
# ModuleNotFoundError: No module named 'mathh'
import mathh  # error

# ImportError: cannot import name 'sine' from 'math'
from math import sine  # error
```

## Rule history

- v1.0.0: Renamed from `unresolved-import`.

  `unresolved-import` remains as a compatibility alias for this rule.
