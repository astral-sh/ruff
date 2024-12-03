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

If you alias `typing.Any` to another name, we still recognize that as a spelling of the Any type.

```py
from typing import Any as RenamedAny

x: RenamedAny = 1
x = "foo"

def f():
    reveal_type(x)  # revealed: Any
```

## Shadowed class

If you define your own class named `Any`, using that in a type expression refers to your class, and
isn't a spelling of the Any type.

```py
class Any:
    pass

x: Any

def f():
    reveal_type(x)  # revealed: Any

# This verifies that we're not accidentally seeing typing.Any, since str is assignable
# to that but not to our locally defined class.
y: Any = "not an Any"  # error: [invalid-assignment]
```

## Subclass

The spec allows you to define subclasses of `Any`, which must also resolve to the Any type.

```py
from typing import Any

class Subclass(Any):
    pass

# Since Subclass is a subclass of Any, it is assignable to and from any other type, just like Any.
x: Subclass = 1
y: int = Subclass()

def f() -> Subclass:
    pass

reveal_type(f())  # revealed: Any
```
