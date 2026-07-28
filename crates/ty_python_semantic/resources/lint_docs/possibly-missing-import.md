## What it does

Checks for imports of symbols that may be missing.

## Why is this bad?

Importing a missing module or name will raise a `ModuleNotFoundError`
or `ImportError` at runtime.

## Rule status

This rule is currently disabled by default because of the number of
false positives it can produce.

## Examples

`module.py`:

```python
import datetime

if datetime.date.today().weekday() != 6:
    a = 1
```

`main.py`:

```python
# ImportError: cannot import name 'a' from 'module'
from module import a  # error
```
