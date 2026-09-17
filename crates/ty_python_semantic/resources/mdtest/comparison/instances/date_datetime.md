# Comparison: `date` and `datetime` mixed operations

`datetime.datetime` inherits from `datetime.date`, but their inherited rich-comparison methods
(`__lt__`, `__le__`, `__gt__`, `__ge__`) and `__sub__` reject mixed `date`/`datetime` operands at
runtime rather than silently comparing/subtracting across the two types.

The exact runtime behavior is more subtle than "always raises", and it changed in Python 3.13:

- The literal `date` and `datetime` classes always raise `TypeError` when mixed, on every Python
  version, for both the ordering comparisons and `__sub__`.
- A genuine **subclass** of `date` (not the literal `date` class itself) compared with a
  `datetime` via `<`, `<=`, `>`, or `>=`, with the `date` subclass instance on the left, does
  **not** raise before Python 3.13 -- it silently returns a `bool` by comparing only the date
  fields, ignoring the `datetime`'s time component. From Python 3.13 onwards, this case raises
  `TypeError` too, closing the gap.
- `__sub__` between a `date` subclass and a `datetime` always raises, on every version -- the
  pre-3.13 exemption above is specific to the four ordering comparisons.
- A subclass that overrides the relevant dunder itself is exempt from all of the above, since ty
  can no longer assume the operation carries `date`/`datetime`'s own unsafe runtime behavior.
- A `NewType` wrapping `date` has no runtime class of its own -- an instance of it *is* a plain
  `date` at runtime, so it behaves like the literal `date` case, not like a genuine subclass.

## Literal `date` and `datetime` are always unsupported

```py
from datetime import date, datetime

d = date(2020, 1, 1)
dt = datetime(2020, 1, 1, 12, 30)

# error: [unsupported-operator] "Operator `<` is not supported between objects of type `date` and `datetime`"
reveal_type(d < dt)  # revealed: Unknown
# error: [unsupported-operator] "Operator `<` is not supported between objects of type `datetime` and `date`"
reveal_type(dt < d)  # revealed: Unknown
# error: [unsupported-operator] "Operator `<=` is not supported between objects of type `date` and `datetime`"
reveal_type(d <= dt)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>` is not supported between objects of type `date` and `datetime`"
reveal_type(d > dt)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>=` is not supported between objects of type `date` and `datetime`"
reveal_type(d >= dt)  # revealed: Unknown

# error: [unsupported-operator] "Operator `-` is not supported between objects of type `date` and `datetime`"
reveal_type(d - dt)  # revealed: Unknown
# error: [unsupported-operator] "Operator `-` is not supported between objects of type `datetime` and `date`"
reveal_type(dt - d)  # revealed: Unknown

# Equality never raises at runtime for mismatched date/datetime, so it must stay unaffected.
reveal_type(d == dt)  # revealed: bool
reveal_type(d != dt)  # revealed: bool
```

## A `date` subclass compared with `datetime` is not flagged before Python 3.13

```toml
[environment]
python-version = "3.12"
```

```py
from datetime import date, datetime

class MyDate(date):
    pass

md = MyDate(2020, 1, 1)
dt = datetime(2020, 1, 1, 12, 30)

# On Python < 3.13, CPython's `date.__lt__` (and `__le__`/`__gt__`/`__ge__`) silently compares
# only the date fields when the left operand is a `date` subclass (not the literal `date`
# class) and the right operand is a `datetime` -- no exception is raised, so this must not be
# flagged.
reveal_type(md < dt)  # revealed: bool
reveal_type(md <= dt)  # revealed: bool
reveal_type(md > dt)  # revealed: bool
reveal_type(md >= dt)  # revealed: bool

# Subtraction between a `date` subclass and a `datetime` always raises `TypeError`, on every
# Python version -- the pre-3.13 exemption above is specific to the ordering comparisons.
# error: [unsupported-operator] "Operator `-` is not supported between objects of type `MyDate` and `datetime`"
reveal_type(md - dt)  # revealed: Unknown
```

## The same subclass comparison is flagged from Python 3.13 onwards

```toml
[environment]
python-version = "3.13"
```

```py
from datetime import date, datetime

class MyDate(date):
    pass

md = MyDate(2020, 1, 1)
dt = datetime(2020, 1, 1, 12, 30)

# CPython 3.13 made `date`-subclass-vs-`datetime` ordering comparisons raise `TypeError`
# consistently, closing the pre-3.13 silent-success gap exercised above.
# error: [unsupported-operator] "Operator `<` is not supported between objects of type `MyDate` and `datetime`"
reveal_type(md < dt)  # revealed: Unknown
# error: [unsupported-operator] "Operator `<=` is not supported between objects of type `MyDate` and `datetime`"
reveal_type(md <= dt)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>` is not supported between objects of type `MyDate` and `datetime`"
reveal_type(md > dt)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>=` is not supported between objects of type `MyDate` and `datetime`"
reveal_type(md >= dt)  # revealed: Unknown
```

## A subclass that overrides the operator itself is exempt regardless of Python version

Subtraction has no pre-3.13 exemption at all (see above), so overriding `__sub__` is the
cleanest way to demonstrate the override escape hatch independently of the target Python
version:

```toml
[environment]
python-version = "3.12"
```

```py
from datetime import date, datetime

class SafeDate(date):
    def __sub__(self, other: object) -> int:
        return 0

sd = SafeDate(2020, 1, 1)
dt = datetime(2020, 1, 1, 12, 30)

# `SafeDate` overrides `__sub__` itself, so ty can no longer assume the operation carries
# `date`'s unsafe inherited runtime behavior -- unlike the plain-subclass subtraction case
# above, this must not be flagged.
reveal_type(sd - dt)  # revealed: int
```

The same holds for the ordering comparisons on Python 3.13, where the plain subclass case
above *is* flagged:

```toml
[environment]
python-version = "3.13"
```

```py
from datetime import date, datetime

class SafeOrderedDate(date):
    def __lt__(self, other: object) -> bool:
        return False

sod = SafeOrderedDate(2020, 1, 1)
dt = datetime(2020, 1, 1, 12, 30)

# Even on 3.13, where the plain-subclass comparison above is flagged, an explicit `__lt__`
# override opts this class out of the unsafe-inherited-method assumption.
reveal_type(sod < dt)  # revealed: bool
```

## `NewType` over `date` behaves like the literal class, not like a subclass

```toml
[environment]
python-version = "3.12"
```

```py
from datetime import date, datetime
from typing import NewType

UserDate = NewType("UserDate", date)

ud = UserDate(date(2020, 1, 1))
dt = datetime(2020, 1, 1, 12, 30)

# `NewType` has no runtime class of its own -- `ud` is a plain `date` instance at runtime, so
# it must be flagged the same way the literal `date` case at the top of this file is, even on
# Python < 3.13 where a genuine subclass would be exempt (see above).
# error: [unsupported-operator] "Operator `<` is not supported between objects of type `date` and `datetime`"
reveal_type(ud < dt)  # revealed: Unknown
```
