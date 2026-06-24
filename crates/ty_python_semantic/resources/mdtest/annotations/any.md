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

Instances of direct and indirect subclasses of `Any` retain their nominal type and declared members,
but are gradually assignable to arbitrary types. This assignability is one-way: an arbitrary value
is not assignable to either subclass.

```py
from typing import Any
from ty_extensions import reveal_mro

class SubclassOfAny(Any): ...
class IndirectSubclass(SubclassOfAny): ...

reveal_mro(SubclassOfAny)  # revealed: (<class 'SubclassOfAny'>, Any, <class 'object'>)
reveal_mro(IndirectSubclass)  # revealed: (<class 'IndirectSubclass'>, <class 'SubclassOfAny'>, Any, <class 'object'>)
reveal_type(SubclassOfAny())  # revealed: SubclassOfAny
reveal_type(IndirectSubclass())  # revealed: IndirectSubclass

not_a_direct_instance: SubclassOfAny = 1  # error: [invalid-assignment]
not_an_indirect_instance: IndirectSubclass = 1  # error: [invalid-assignment]
direct_as_int: int = SubclassOfAny()
indirect_as_int: int = IndirectSubclass()
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

This behavior is preserved through dynamically created subclasses:

```py
from typing import Any, final
from ty_extensions import reveal_mro

class A(Any): ...

B = type("B", (A,), {})

class C(B): ...

@final
class FinalClass: ...

reveal_mro(B)  # revealed: (<class 'B'>, <class 'A'>, Any, <class 'object'>)
reveal_mro(C)  # revealed: (<class 'C'>, <class 'B'>, <class 'A'>, Any, <class 'object'>)

b: FinalClass = B()
c: FinalClass = C()
```

`Annotated` metadata on a base does not change this behavior:

```py
from typing import Annotated, Any, Literal, final
from ty_extensions import reveal_mro

class A(Any): ...
class B(Annotated[A, "metadata"]): ...

@final
class FinalClass: ...

reveal_mro(B)  # revealed: (<class 'B'>, <class 'A'>, Any, <class 'object'>)

x: FinalClass = B()
y: Literal[1] = B()
```

This behavior is also preserved when the subclass inherits from a generic alias:

```py
from typing import Any, Generic, Literal, TypeVar, final

T = TypeVar("T")

class GenericSubclass(Any, Generic[T]): ...
class SubclassOfGenericAlias(GenericSubclass[int]): ...

@final
class FinalClass: ...

final: FinalClass = SubclassOfGenericAlias()
literal: Literal[1] = SubclassOfGenericAlias()
```

A base expression whose inferred type is `Any` does not count as explicitly inheriting from `Any`.
The dynamic MRO entry makes instances assignable to non-final classes, but unlike an explicit `Any`
base, not to final or literal types:

```py
from typing import Any, Literal, final

class Arbitrary: ...

@final
class FinalClass: ...

def check_dynamic_base(base: Any):
    class DynamicBase(base): ...
    class IndirectSubclass(DynamicBase): ...

    reveal_type(DynamicBase())  # revealed: DynamicBase
    ordinary: Arbitrary = IndirectSubclass()
    final: FinalClass = DynamicBase()  # error: [invalid-assignment]
    literal: Literal[1] = DynamicBase()  # error: [invalid-assignment]
    indirect_final: FinalClass = IndirectSubclass()  # error: [invalid-assignment]
    indirect_literal: Literal[1] = IndirectSubclass()  # error: [invalid-assignment]
```

Similarly, inheriting from a name whose inferred type is `Unknown` makes instances assignable to
non-final classes, but not to final classes:

```py
from typing import final

from somewhere import UnknownBase  # error: [unresolved-import]

class Arbitrary: ...

@final
class FinalClass: ...

class FromUnknownBase(UnknownBase): ...

reveal_type(FromUnknownBase())  # revealed: FromUnknownBase
ordinary_unknown: Arbitrary = FromUnknownBase()
final_unknown: FinalClass = FromUnknownBase()  # error: [invalid-assignment]
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

Error context from a failed structural check should be discarded when assignability succeeds due to
an explicit `Any` base:

```py
from typing import Any, Callable

class CallableSubclassOfAny(Any):
    def __call__(self, x: int) -> str:
        raise NotImplementedError

class IncompatibleCallable:
    def __call__(self, x: int) -> bytes:
        raise NotImplementedError

def check_callable_union(value1: CallableSubclassOfAny | IncompatibleCallable):
    target1: Callable[[int], int] = value1  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `CallableSubclassOfAny | IncompatibleCallable` is not assignable to `(int, /) -> int`
   --> src/mdtest_snippet.py:141:14
    |
141 |     target1: Callable[[int], int] = value1  # snapshot
    |              --------------------   ^^^^^^ Incompatible value of type `CallableSubclassOfAny | IncompatibleCallable`
    |              |
    |              Declared type
    |
info: element `IncompatibleCallable` of union `CallableSubclassOfAny | IncompatibleCallable` is not assignable to `(int, /) -> int`
info: └── type `IncompatibleCallable` has inferred callable type `(x: int) -> bytes`
info:     └── incompatible return types: `bytes` is not assignable to `int`
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
