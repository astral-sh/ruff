# relative-imports (TID252)

Derived from the **flake8-tidy-imports** linter.

Autofix is sometimes available.

## What it does
Checks for relative imports.

## Why is this bad?
Absolute imports, or relative imports from siblings, are recommended by [PEP 8](https://peps.python.org/pep-0008/#imports):

> Absolute imports are recommended, as they are usually more readable and tend to be better behaved...
> ```python
> import mypkg.sibling
> from mypkg import sibling
> from mypkg.sibling import example
> ```
> However, explicit relative imports are an acceptable alternative to absolute imports,
> especially when dealing with complex package layouts where using absolute imports would be
> unnecessarily verbose:
> ```python
> from . import sibling
> from .sibling import example
> ```

Note that degree of strictness packages can be specified via the
[`ban-relative-imports`](https://github.com/charliermarsh/ruff#ban-relative-imports)
configuration option, which allows banning all relative imports
(`ban-relative-imports = "all"`) or only those that extend into the parent module or beyond
(`ban-relative-imports = "parents"`, the default).

## Example
```python
from .. import foo
```

Use instead:
```python
from mypkg import foo
```