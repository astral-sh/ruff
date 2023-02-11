# missing-required-import (I002)

Derived from the **isort** linter.

Autofix is always available.

## What it does
Adds any required imports, as specified by the user, to the top of the file.

## Why is this bad?
In some projects, certain imports are required to be present in all files. For
example, some projects assume that `from __future__ import annotations` is enabled,
and thus require that import to be present in all files. Omitting a "required" import
(as specified by the user) can cause errors or unexpected behavior.

## Example
```python
import typing
```

Use instead:
```python
from __future__ import annotations

import typing
```