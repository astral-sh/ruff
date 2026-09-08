# `typing.TypeForm`

These examples use `unused-import` (`F401`) to check whether quoted type expressions mark their imports
as used.

```toml
target-version = "py315"

[lint]
select = ["F401"]
```

## Subscription and call

Both `TypeForm[...]` and `TypeForm(...)` accept type expressions, so quoted types in either form mark
their imports as used even outside an annotation.

```py
from decimal import Decimal  # no diagnostic
from fractions import Fraction  # no diagnostic
from typing import TypeForm

DecimalForm = TypeForm["Decimal"]
form = TypeForm("Fraction")
```

## `typing_extensions`

Both forms also work with the backport, including when imported under an alias.

```py
from decimal import Decimal  # no diagnostic
from pathlib import Path  # no diagnostic
from typing_extensions import TypeForm as TF

DecimalForm = TF["Decimal"]
form = TF("Path")
```
