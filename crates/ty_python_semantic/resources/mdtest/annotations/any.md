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

Instances of `SubclassOfAny` have type `SubclassOfAny & Any`. The `SubclassOfAny` element preserves
the class's known members, while the `Any` element makes its instances assignable to arbitrary
types. Values of other types are not assignable to `SubclassOfAny`.

```py
from typing import Any
from ty_extensions import reveal_mro

class SubclassOfAny(Any): ...
class IndirectSubclass(SubclassOfAny): ...

reveal_mro(SubclassOfAny)  # revealed: (<class 'SubclassOfAny'>, Any, <class 'object'>)
reveal_type(SubclassOfAny())  # revealed: SubclassOfAny & Any
reveal_type(IndirectSubclass())  # revealed: IndirectSubclass & Any

x: SubclassOfAny = 1  # error: [invalid-assignment]
y: int = SubclassOfAny()
```

This includes final classes:

```py
from typing import final

@final
class FinalClass: ...

f: FinalClass = SubclassOfAny()

@final
class OtherFinalClass: ...

f: FinalClass | OtherFinalClass = SubclassOfAny()
```

A class with a base whose type is `Any` or `Unknown` is different. Its instances have the ordinary
nominal type of the class and are not assignable to arbitrary types:

```py
from ty_extensions import Unknown

class Arbitrary: ...

def check_dynamic_base(any_base: Any):
    class FromAnyValue(any_base): ...
    class IndirectDynamicSubclass(FromAnyValue): ...
    class FromUnknown(Unknown): ...

    reveal_type(FromAnyValue())  # revealed: FromAnyValue
    reveal_type(IndirectDynamicSubclass())  # revealed: IndirectDynamicSubclass
    reveal_type(FromUnknown())  # revealed: FromUnknown

    x: Arbitrary = FromAnyValue()  # error: [invalid-assignment]
    y: Arbitrary = IndirectDynamicSubclass()  # error: [invalid-assignment]
    z: Arbitrary = FromUnknown()  # error: [invalid-assignment]
```

A subclass of `Any` can also be assigned to arbitrary `Callable` and `Protocol` types:

```py
from typing import Callable, Any, Protocol

def takes_callable1(f: Callable[..., Any]):
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

A subclass of `Any` is also assignable to literal types through the dynamic element of its instance
type:

```py
from typing import Any, Literal

class MockAny(Any):
    pass

x: Literal[1] = MockAny()
```

A use case where subclasses of `Any` come up is in mocking libraries, where the mock object should
be assignable to (almost) any type:

```py
from unittest.mock import MagicMock

x: int = MagicMock()
```

## Runtime properties

`typing.Any` is a class at runtime on Python 3.11+, and `typing_extensions.Any` is always a class.
On earlier versions of Python, `typing.Any` was an instance of `typing._SpecialForm`, but this is
not currently modeled by ty. We currently infer `Any` has having all attributes a class would have
on all versions of Python:

```py
from typing import Any
from ty_extensions import TypeOf, static_assert, is_assignable_to

reveal_type(Any.__base__)  # revealed: type | None
reveal_type(Any.__bases__)  # revealed: tuple[type, ...]
static_assert(is_assignable_to(TypeOf[Any], type))
```

## Invalid

`Any` cannot be parameterized:

```py
from typing import Any

# error: [invalid-type-form] "Special form `typing.Any` expected no type parameter"
def f(x: Any[int]):
    reveal_type(x)  # revealed: Unknown
```

`Any` cannot be called (this leads to a `TypeError` at runtime):

```py
Any()  # error: [call-non-callable] "Object of type `<special-form 'typing.Any'>` is not callable"
```

`Any` also cannot be used as a metaclass (under the hood, this leads to an implicit call to `Any`):

```py
class F(metaclass=Any): ...  # error: [invalid-metaclass] "Metaclass type `<special-form 'typing.Any'>` is not callable"
```

And `Any` cannot be used in `isinstance()` checks:

```py
# error: [invalid-argument-type] "`typing.Any` cannot be used with `isinstance()`: This call will raise `TypeError` at runtime"
isinstance("", Any)
```

The same applies when `Any` is nested inside a tuple, including non-literal tuples:

```py
isinstance("", (int, Any))  # error: [invalid-argument-type]
isinstance("", (int, (str, Any)))  # error: [invalid-argument-type]
classes = (int, Any)
isinstance("", classes)  # error: [invalid-argument-type]
```

But `issubclass()` checks are fine:

```py
issubclass(object, Any)  # no error!
issubclass(object, (int, Any))  # no error!
issubclass(object, (int, (str, Any)))  # no error!
```
