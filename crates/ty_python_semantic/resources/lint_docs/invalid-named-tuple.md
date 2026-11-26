## What it does

Checks for invalid definitions of `typing.NamedTuple` and `collections.namedtuple` classes.

## Why is this bad?

An invalidly defined `NamedTuple` class may lead to the type checker drawing incorrect conclusions.
It may also lead to errors at runtime, such as `ValueError`, `TypeError`, or `AttributeError`.

## Examples

A class definition cannot combine `NamedTuple` with other base classes in multiple inheritance;
doing so raises a `TypeError` at runtime. The sole exception to this rule is `Generic[]`, which can
be used alongside `NamedTuple` in a class's bases list.

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

Nor can they be keywords:

```pycon
>>> from collections import namedtuple
>>> X = namedtuple("X", ["class"])
ValueError: Type names and field names cannot be a keyword: 'class'
```

or invalid identifiers:

```pycon
>>> from collections import namedtuple
>>> X = namedtuple("X", ["-"])
ValueError: Type names and field names must be valid identifiers: '-
```

Similar restrictions apply to the `typename` argument:

```pycon
>>> from typing import NamedTuple
>>> X = NamedTuple("class", [])
ValueError: Type names and field names cannot be a keyword: 'class'
>>> X = NamedTuple("x-", [])
ValueError: Type names and field names must be valid identifiers: 'x-'
```

`NamedTuple` classes also have certain synthesized attributes (like `_asdict`, `_make`, `_replace`,
etc.) that cannot be overwritten. Attempting to assign to these attributes without a type annotation
will raise an `AttributeError` at runtime.

```pycon
>>> from typing import NamedTuple
>>> class Foo(NamedTuple):
...     x: int
...     _asdict = 42
AttributeError: Cannot overwrite NamedTuple attribute _asdict
```

Finally, `NamedTuple` field annotations cannot use the `ClassVar` or `Final` type qualifiers. These
qualifiers also cause a runtime error when annotations are evaluated eagerly:

```pycon
>>> from typing import ClassVar, NamedTuple
>>> class Foo(NamedTuple):
...     x: ClassVar[int]
TypeError: typing.ClassVar[int] is not valid as type argument
```
