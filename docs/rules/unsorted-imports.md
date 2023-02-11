# unsorted-imports (I001)

Derived from the **isort** linter.

Autofix is always available.

## What it does
De-duplicates, groups, and sorts imports based on the provided `isort` settings.

## Why is this bad?
Consistency is good. Use a common convention for imports to make your code
more readable and idiomatic.

## Example
```python
import pandas
import numpy as np
```

Use instead:
```python
import numpy as np
import pandas
```