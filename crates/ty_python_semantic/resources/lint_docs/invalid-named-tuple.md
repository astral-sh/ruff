## What it does

Checks for invalidly defined `NamedTuple` classes.

## Why is this bad?

An invalidly defined `NamedTuple` class may lead to the type checker
drawing incorrect conclusions. It may also lead to `TypeError`s or
`AttributeError`s at runtime.

## Examples

A class definition cannot combine `NamedTuple` with other base classes
in multiple inheritance; doing so raises a `TypeError` at runtime. The sole
exception to this rule is `Generic[]`, which can be used alongside `NamedTuple`
in a class's bases list.

```pycon
>>> from typing import NamedTuple
>>> class Foo(NamedTuple, object): ...
TypeError: can only inherit from a NamedTuple type and Generic
```

Further, `NamedTuple` field names cannot start with an underscore:

```pycon
>>> from typing import NamedTuple
>>> class Foo(NamedTuple):
...     _bar: int
ValueError: Field names cannot start with an underscore: '_bar'
```

`NamedTuple` classes also have certain synthesized attributes (like `_asdict`, `_make`,
`_replace`, etc.) that cannot be overwritten. Attempting to assign to these attributes
without a type annotation will raise an `AttributeError` at runtime.

```pycon
>>> from typing import NamedTuple
>>> class Foo(NamedTuple):
...     x: int
...     _asdict = 42
AttributeError: Cannot overwrite NamedTuple attribute _asdict
```
