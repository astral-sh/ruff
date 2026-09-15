# `unused-import` (`F401`)

```toml
target-version = "py315"

[lint]
select = ["F401"]
```

## Standard-library generic types

For standard-library generic types such as `slice` and `frozendict`, Ruff visits subscript arguments as
type expressions. Names in quoted annotations therefore mark their imports as used.

```py
from decimal import Decimal  # no diagnostic
from fractions import Fraction  # no diagnostic

DecimalSlice = slice["Decimal"]
FractionMap = frozendict[str, "Fraction"]
```

## Other subscription arguments

For an arbitrary subscription such as `foo["Decimal"]`, the string argument is not treated as a type
expression. The import of `Decimal` is therefore unused.

```py
from decimal import Decimal  # error: [unused-import]

from somewhere import foo

NotAnAlias = foo["Decimal"]
```
