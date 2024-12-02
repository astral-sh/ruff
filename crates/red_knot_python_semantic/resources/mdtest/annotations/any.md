# Any

## Annotation

`typing.Any` is a way to name the Any type.

```py
from typing import Any

x: Any = 1
x = "foo"

def f():
    reveal_type(x)  # revealed: Any
```

## Aliased to a different name

If you alias `typing.Any` to another name, we still recognize that as a spelling
of the Any type.

```py
from typing import Any as RenamedAny

x: RenamedAny = 1
x = "foo"

def f():
    reveal_type(x)  # revealed: Any
```

## Shadowed class

If you define your own class named `Any`, using that in a type expression refers
to your class, and isn't a spelling of the Any type.

> Note that the real name of the class shouldn't be `Any`, so that we can
> distinguish it from the Any type in the assertion below.

```py
class LocalAny:
    pass

Any = LocalAny

x: Any = Any()

def f():
    reveal_type(x)  # revealed: LocalAny
```
