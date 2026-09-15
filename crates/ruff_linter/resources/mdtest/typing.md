# Typing constructs

These examples use `unused-import` (`F401`) to check whether quoted type expressions mark their imports
as used.

```toml
target-version = "py315"

[lint]
select = ["F401"]
```

## `TypeForm` subscription and call

Both `TypeForm[...]` and `TypeForm(...)` accept type expressions, so quoted types in either form mark
their imports as used even outside an annotation.

```py
from decimal import Decimal  # no diagnostic
from fractions import Fraction  # no diagnostic
from typing import TypeForm

DecimalForm = TypeForm["Decimal"]
form = TypeForm("Fraction")
```

## `TypeForm` backport

Both forms also work with the backport, including when imported under an alias.

```py
from decimal import Decimal  # no diagnostic
from pathlib import Path  # no diagnostic
from typing_extensions import TypeForm as TF

DecimalForm = TF["Decimal"]
form = TF("Path")
```

## `TypeIs` and `TypeGuard`

Quoted types in `TypeIs` and `TypeGuard` subscriptions also mark their imports as used, even outside
annotations. This includes the backports in `typing_extensions`.

```py
from decimal import Decimal  # no diagnostic
from fractions import Fraction  # no diagnostic
from pathlib import Path  # no diagnostic
from typing import TypeIs
import typing_extensions

IsDecimal = TypeIs["Decimal"]
IsFraction = typing_extensions.TypeIs["Fraction"]
GuardsPath = typing_extensions.TypeGuard["Path"]
```
