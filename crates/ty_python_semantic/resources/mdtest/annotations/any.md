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

## Subclasses of `Any`

The spec allows you to define subclasses of `Any`.

`SubclassOfAny` has an unknown superclass, which might be `int`. The assignment to `x` should not be
allowed, even when the unknown superclass is `int`. The assignment to `y` should be allowed, since
`Subclass` might have `int` as a superclass, and is therefore assignable to `int`.

```py
from typing import Any

class SubclassOfAny(Any): ...

reveal_type(SubclassOfAny.__mro__)  # revealed: tuple[<class 'SubclassOfAny'>, Any, <class 'object'>]

x: SubclassOfAny = 1  # error: [invalid-assignment]
y: int = SubclassOfAny()
```

`SubclassOfAny` should not be assignable to a final class though, because `SubclassOfAny` could not
possibly be a subclass of `FinalClass`:

```py
from typing import final

@final
class FinalClass: ...

f: FinalClass = SubclassOfAny()  # error: [invalid-assignment]

@final
class OtherFinalClass: ...

f: FinalClass | OtherFinalClass = SubclassOfAny()  # error: [invalid-assignment]
```

A subclass of `Any` can also be assigned to arbitrary `Callable` and `Protocol` types:

```py
from typing import Callable, Any, Protocol

def takes_callable1(f: Callable):
    f()

takes_callable1(SubclassOfAny())

def takes_callable2(f: Callable[[int], None]):
    f(1)

takes_callable2(SubclassOfAny())

class CallbackProtocol(Protocol):
    def __call__(self, x: int, /) -> None: ...

def takes_callback_proto(f: CallbackProtocol):
    f(1)

takes_callback_proto(SubclassOfAny())

class OtherProtocol(Protocol):
    x: int
    @property
    def foo(self) -> bytes: ...
    @foo.setter
    def foo(self, x: str) -> None: ...

def takes_other_protocol(f: OtherProtocol): ...

takes_other_protocol(SubclassOfAny())
```

A subclass of `Any` cannot be assigned to literal types, since those can not be subclassed:

```py
from typing import Any, Literal

class MockAny(Any):
    pass

x: Literal[1] = MockAny()  # error: [invalid-assignment]
```

A use case where subclasses of `Any` come up is in mocking libraries, where the mock object should
be assignable to (almost) any type:

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
