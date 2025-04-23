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
class Any: ...

x: Any

def f():
    reveal_type(x)  # revealed: Any

# This verifies that we're not accidentally seeing typing.Any, since str is assignable
# to that but not to our locally defined class.
y: Any = "not an Any"  # error: [invalid-assignment]
```

## Subclass

The spec allows you to define subclasses of `Any`.

`Subclass` has an unknown superclass, which might be `int`. The assignment to `x` should not be
allowed, even when the unknown superclass is `int`. The assignment to `y` should be allowed, since
`Subclass` might have `int` as a superclass, and is therefore assignable to `int`.

```py
from typing import Any

class Subclass(Any): ...

reveal_type(Subclass.__mro__)  # revealed: tuple[Literal[Subclass], Any, Literal[object]]

x: Subclass = 1  # error: [invalid-assignment]
y: int = Subclass()

def _(s: Subclass):
    reveal_type(s)  # revealed: Subclass
```

`Subclass` should not be assignable to a final class though, because `Subclass` could not possibly
be a subclass of `FinalClass`:

```py
from typing import final

@final
class FinalClass: ...

f: FinalClass = Subclass()  # error: [invalid-assignment]
```

A use case where this comes up is with mocking libraries, where the mock object should be assignable
to any type:

```py
from unittest.mock import MagicMock

x: int = MagicMock()
```

## Invalid

`Any` cannot be parameterized:

```py
from typing import Any

# error: [invalid-type-form] "Type `typing.Any` expected no type parameter"
def f(x: Any[int]):
    reveal_type(x)  # revealed: Unknown
```
