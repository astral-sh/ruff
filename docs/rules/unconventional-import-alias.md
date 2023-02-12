# unconventional-import-alias (ICN001)

Derived from the **flake8-import-conventions** linter.

## What it does
Checks for imports that are typically imported using a common convention,
like `import pandas as pd`, and enforces that convention.

## Why is this bad?
Consistency is good. Use a common convention for imports to make your code
more readable and idiomatic.

For example, `import pandas as pd` is a common
convention for importing the `pandas` library, and users typically expect
Pandas to be aliased as `pd`.

## Example
```python
import pandas
```

Use instead:
```python
import pandas as pd
```