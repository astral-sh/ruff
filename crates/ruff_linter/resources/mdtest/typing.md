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

## `TypedDict` extra items

The `extra_items` argument to a `TypedDict` class is a type expression. Imports referenced only in
this quoted argument are used.

```py
from decimal import Decimal  # no diagnostic
from typing import TypedDict

class Record(TypedDict, extra_items="Decimal"):
    pass
```

## `TypedDict` extra items from `typing_extensions`

The backported `TypedDict` also accepts quoted types in `extra_items`, including when imported under
an alias.

```py
from decimal import Decimal  # no diagnostic
from typing_extensions import TypedDict as TD

class Record(TD, extra_items="Decimal"):
    pass
```

## Inherited `TypedDict` extra items

Subclasses can specify the type of extra items, including when their base is a generic `TypedDict`.

```py
from decimal import Decimal  # no diagnostic
from typing import TypedDict

class Base[T](TypedDict):
    value: T

class Record(Base[int], extra_items="Decimal"):
    pass
```

## Functional `TypedDict` base

A class can also inherit from a `TypedDict` defined with the functional syntax. Its `extra_items`
argument is still a type expression.

```py
from decimal import Decimal  # no diagnostic
from typing import TypedDict

Base = TypedDict("Base", {})

class Record(Base, extra_items="Decimal"):
    pass
```

## Forward `TypedDict` base in a stub

Stub files allow a base class to be defined later. Quoted extra item types still use their imports
when the base is a `TypedDict`.

```pyi
from decimal import Decimal  # no diagnostic
from typing import TypedDict

class Record(Base, extra_items="Decimal"): ...

class Base(TypedDict): ...
```

## Extra items on an unrelated class

A class keyword named `extra_items` is not a type expression unless the class inherits from
`TypedDict`.

```py
from decimal import Decimal  # error: [unused-import] "`decimal.Decimal` imported but unused"

class Record(extra_items="Decimal"):
    pass
```
