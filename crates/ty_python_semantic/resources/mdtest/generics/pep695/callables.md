# Generic callables: PEP 695 syntax

```toml
[environment]
python-version = "3.12"
```

## Callables can be generic

Many items that are callable can also be generic. Generic functions are the most obvious example:

```py
from typing import Callable
from ty_extensions._internal import generic_context

def identity[T](t: T) -> T:
    return t

# revealed: ty_extensions._internal.GenericContext[T@identity]
reveal_type(generic_context(identity))
# revealed: Literal[1]
reveal_type(identity(1))

def identity2[**P, T](c: Callable[P, T]) -> Callable[P, T]:
    return c

# revealed: ty_extensions._internal.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(identity2))
# revealed: [T](t: T) -> T
reveal_type(identity2(identity))

class CallableInstance:
    def __call__(self, value: int, /) -> str:
        return str(value)

# revealed: (value: int, /) -> str
reveal_type(identity2(CallableInstance()))
```

Generic classes are another example, since you invoke the class to instantiate it:

```py
class C[T]:
    t: T  # invariant

    def __init__(self, t: T) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: C[int]
reveal_type(C(1))
```

Explicit generic receiver annotations constrain a bound method's callable type:

```py
from typing import Callable

class GenericReceiver:
    def method[T](self: T, value: T) -> T:
        return self

receiver = GenericReceiver()

# Binding adds `GenericReceiver <= T`. `T = object` satisfies that constraint, but `T = int` does
# not.
accepts_object: Callable[[object], object] = receiver.method
accepts_int: Callable[[int], int] = receiver.method  # error: [invalid-assignment]
```

The receiver must also satisfy a method type variable's declared bound or constraints:

```py
from typing import Callable

class InvalidBoundedReceiver:
    def method[T: int](self: T) -> None: ...

class ValidBoundedReceiver(int):
    def method[T: int](self: T) -> None: ...

class InvalidConstrainedReceiver:
    def method[T: (int, str)](self: T) -> None: ...

class ValidConstrainedReceiver(str):
    def method[T: (int, str)](self: T) -> None: ...

type ReceiverAlias[T] = T

class InvalidAliasedBoundedReceiver:
    def method[T: int](self: ReceiverAlias[T]) -> None: ...

class InvalidNestedBoundedReceiver(list[str]):
    def method[T: int](self: list[T]) -> None: ...

class InvalidUnionConstrainedReceiver:
    def method[T: (int, str)](self: T | None) -> None: ...

invalid_bound: Callable[[], None] = InvalidBoundedReceiver().method  # error: [invalid-assignment]
valid_bound: Callable[[], None] = ValidBoundedReceiver().method

invalid_constraints: Callable[[], None] = InvalidConstrainedReceiver().method  # error: [invalid-assignment]
valid_constraints: Callable[[], None] = ValidConstrainedReceiver().method

invalid_aliased_bound: Callable[[], None] = InvalidAliasedBoundedReceiver().method  # error: [invalid-assignment]

# TODO: Enforce valid specializations for TypeVars nested inside receiver annotations.
invalid_nested_bound: Callable[[], None] = InvalidNestedBoundedReceiver().method  # TODO: error: [invalid-assignment]
invalid_union_constraints: Callable[[], None] = InvalidUnionConstrainedReceiver().method  # TODO: error: [invalid-assignment]
```

When we coerce a generic callable into a `Callable` type, it remembers that it is generic:

```py
from ty_extensions._internal import into_regular_callable

# revealed: [T](t: T) -> T
reveal_type(into_regular_callable(identity))
# revealed: ty_extensions._internal.GenericContext[T@identity]
reveal_type(generic_context(into_regular_callable(identity)))
# revealed: Literal[1]
reveal_type(into_regular_callable(identity)(1))

# revealed: [**P, T](c: (**P) -> T) -> ((**P) -> T)
reveal_type(into_regular_callable(identity2))
# revealed: ty_extensions._internal.GenericContext[P@identity2, T@identity2]
reveal_type(generic_context(into_regular_callable(identity2)))
# revealed: [T](t: T) -> T
reveal_type(into_regular_callable(identity2)(identity))

# revealed: [T](t: T) -> C[T]
reveal_type(into_regular_callable(C))
# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))
# revealed: C[int]
reveal_type(into_regular_callable(C)(1))
```

## Constructor callbacks with receiver-specific overloads

In the below example, the applicable `__new__` overload for `Factory[int]` returns a `Factory[int]`,
so construction also calls `__init__`. Callback compatibility therefore requires its `int` argument.
The overload that returns `str` applies only to `Factory[str]` and cannot bypass this requirement.

```py
from __future__ import annotations
from typing import Callable, overload

class Factory[T]:
    value: T

    @overload
    def __new__(cls: type[Factory[int]], *args: object) -> Factory[int]: ...
    @overload
    def __new__(cls: type[Factory[str]], *args: object) -> str: ...
    def __new__(cls, *args: object) -> Factory[int] | str:
        raise NotImplementedError

    def __init__(self, value: int) -> None: ...

valid: Callable[[int], Factory[int]] = Factory[int]
missing_argument: Callable[[], Factory[int]] = Factory[int]  # error: [invalid-assignment]
wrong_argument: Callable[[str], Factory[int]] = Factory[int]  # error: [invalid-assignment]
```

## Recursive constructors with growing type arguments

In the example below, resolving either constructor repeatedly nests its type argument inside another
`list`. Both direct calls and callback assignments stop expanding these recursive constructors
rather than overflowing the stack.

```py
from typing import Callable

class New[T]:
    __new__: "type[New[list[T]]]"

class Init[T]:
    __init__: "type[Init[list[T]]]"

new: Callable[..., object] = New[int]
init: Callable[..., Init[int]] = Init[int]

reveal_type(New[int]())  # revealed: New[int]
reveal_type(Init[int]())  # revealed: Init[int]
```

## Recursive constructors with growing variadic arguments

In the example below, each recursive initializer appends an element to a type parameter tuple or
prepends a parameter to a `ParamSpec`. Callable conversion and direct calls stop expanding these
chains. The alternative initializer still requires an integer, so a callback cannot omit it.

```py
from typing import Callable, Concatenate

class End:
    def __init__(self, value: int) -> None: ...

class Variadic[*Ts]:
    __init__: "type[Variadic[*Ts, int]] | type[End]"

class Parameters[**P]:
    __init__: "type[Parameters[Concatenate[int, P]]] | type[End]"

valid_variadic: Callable[[int], Variadic[str]] = Variadic[str]
invalid_variadic: Callable[[], Variadic[str]] = Variadic[str]  # error: [invalid-assignment]
valid_paramspec: Callable[[int], Parameters[[str]]] = Parameters[[str]]
invalid_paramspec: Callable[[], Parameters[[str]]] = Parameters[[str]]  # error: [invalid-assignment]

reveal_type(Variadic[str](1))  # revealed: Variadic[str]
reveal_type(Parameters[[str]](1))  # revealed: Parameters[(str, /)]
```

## Recursive constructors passed through type parameters

In the example below, the forwarding class calls whichever constructor it receives as its type
argument. Expanding both nested uses of this helper exposes a growing constructor, whether the
helper is used as `__new__` or `__init__`. Direct calls and callback assignments stop expanding the
resulting cycles.

```py
from typing import Callable

class Forward[T]:
    __new__: type[T]

class Grow[T]:
    __new__: "type[Forward[Forward[Grow[list[T]]]]]"

class Init[T]:
    __init__: "type[Forward[Forward[Init[list[T]]]]]"

callback: Callable[..., object] = Grow[int]
initializer: Callable[..., Init[int]] = Init[int]
reveal_type(Grow[int]())  # revealed: Grow[int]
reveal_type(Init[int]())  # revealed: Init[int]
```

## Finite initializer chains with unused type parameters

In the example below, each initializer forwards to its first type argument. The second parameter on
`Forward` is unused, so changing it does not alter the finite chain. The class's callback signature
retains the final initializer's required integer parameter.

```py
from typing import Callable

class End:
    def __init__(self, value: int) -> None: ...

class Forward[A, B]:
    __init__: type[A]

class C[T]:
    __init__: type[Forward[Forward[T, int], str]]

valid: Callable[[int], C[C[C[End]]]] = C[C[C[End]]]
missing: Callable[[], C[C[C[End]]]] = C[C[C[End]]]  # error: [invalid-assignment]
wrong: Callable[[str], C[C[C[End]]]] = C[C[C[End]]]  # error: [invalid-assignment]
```

## Finite `__new__` chains with unused type parameters

In the example below, each `__new__` forwards to its first type argument while the unused second
argument changes. The finite chain reaches a constructor that returns an integer rather than an
instance of the outer class. Direct calls retain its required keyword argument, and callback
assignments retain its return type.

```py
from typing import Callable

class End:
    def __new__(cls, *args: object, value: int) -> int:
        return value

class Forward[A, B]:
    __new__: type[A]

class C[T]:
    __new__: type[Forward[Forward[T, int], str]]

reveal_type(C[C[C[End]]](value=1))  # revealed: int
C[C[C[End]]]()  # error: [missing-argument]
C[C[C[End]]](value="bad")  # error: [invalid-argument-type]

valid: Callable[..., int] = C[C[C[End]]]
wrong_result: Callable[..., str] = C[C[C[End]]]  # error: [invalid-assignment]
```

## Finite initializer chains with arguments changing in opposite directions

In the example below, each recursive step removes a `list` from the first argument and wraps the
second in a `list`. The first argument eventually reaches `int`, where the descriptor selects `End`.
Growth in the second argument does not prevent this finite chain from retaining the initializer's
required integer parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer:
    @overload
    def __get__[U](self, instance: "C[int, U]", owner: type) -> type[End]: ...
    @overload
    def __get__[T, U](self, instance: "C[list[T], U]", owner: type) -> "type[C[T, list[U]]]": ...
    @overload
    def __get__[T, U](self, instance: "C[T, U]", owner: type) -> "type[C[T, list[U]]] | type[End]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T, U]:
    first: T
    second: U
    __init__ = Initializer()

def check(cls: type[C[list[list[int]], str]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[list[list[int]], str]] = cls
    missing: Callable[[], C[list[list[int]], str]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[list[list[int]], str]] = cls  # error: [invalid-assignment]
```

## Finite constructor chains selected by descriptors

In the example below, the initializer descriptor selects the finite chain
`C[int] -> C[list[int]] -> C[list[list[int]]] -> End`. Its type argument grows temporarily, but each
step selects a different overload with a fixed return type. Other specializations can grow
recursively through the generic overload. Direct calls and callback assignments retain the final
initializer's required integer parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer:
    @overload
    def __get__(self, instance: "C[int]", owner: type) -> "type[C[list[int]]]": ...
    @overload
    def __get__(self, instance: "C[list[int]]", owner: type) -> "type[C[list[list[int]]]]": ...
    @overload
    def __get__(self, instance: "C[list[list[int]]]", owner: type) -> type[End]: ...
    @overload
    def __get__[T](self, instance: "C[T]", owner: type) -> "type[C[list[T]]] | type[End]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T]:
    value: T
    __init__ = Initializer()

def check(cls: type[C[int]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[int]] = cls
    missing: Callable[[], C[int]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[int]] = cls  # error: [invalid-assignment]
```

## Finite constructor chains selected by metaclass descriptors

In the example below, a metaclass descriptor selects the finite chain
`C[int] -> C[list[int]] -> End`. Resolving `__call__` retains the overload selected for each class,
so temporary growth in the type argument does not lose the final constructor's required integer
parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class CallDescriptor:
    @overload
    def __get__(self, instance: "type[C[list[int]]]", owner: type) -> type[End]: ...
    @overload
    def __get__[T](self, instance: "type[C[T]]", owner: type) -> "type[C[list[T]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class Meta(type):
    __call__ = CallDescriptor()

class C[T](metaclass=Meta):
    value: T

def check(cls: type[C[int]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], End] = cls
    missing: Callable[[], End] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], End] = cls  # error: [invalid-assignment]
```

## Finite constructor chains through generic descriptor overloads

In the example below, the initializer descriptor selects `C[list[int]] -> C[set[int]] -> End`. The
generic overloads preserve the element type while changing the outer container. Although the
argument does not shrink, this chain terminates and retains the required integer parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer:
    @overload
    def __get__[T](self, instance: "C[list[T]]", owner: type) -> "type[C[set[T]]]": ...
    @overload
    def __get__[T](self, instance: "C[set[T]]", owner: type) -> type[End]: ...
    @overload
    def __get__[T](self, instance: "C[T]", owner: type) -> "type[C[list[T]]] | Any": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T]:
    value: T
    __init__ = Initializer()

def check(cls: type[C[list[int]]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[list[int]]] = cls
    missing: Callable[[], C[list[int]]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[list[int]]] = cls  # error: [invalid-assignment]
```

## Finite constructor chains repeating a generic descriptor overload

In the example below, the generic descriptor overload wraps the type argument in a `list` three
times before the more specific overload selects `End`. Repeating the generic overload does not make
the chain infinite. Direct calls and callback assignments retain the final initializer's required
integer parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer:
    @overload
    def __get__(self, instance: "C[list[list[list[int]]]]", owner: type) -> type[End]: ...
    @overload
    def __get__[T](self, instance: "C[T]", owner: type) -> "type[C[list[T]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T]:
    value: T
    __init__ = Initializer()

def check(cls: type[C[int]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[int]] = cls
    missing: Callable[[], C[int]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[int]] = cls  # error: [invalid-assignment]
```

## Finite constructor chains that reorder type arguments

In the example below, the initializer rotates a tuple until `str` reaches its first position. Each
step preserves the tuple's size but changes which overload will eventually match. Direct calls and
callback assignments retain the final initializer's required integer parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer:
    @overload
    def __get__(self, instance: "C[tuple[str, int, int, int]]", owner: type) -> type[End]: ...
    @overload
    def __get__[W, X, Y, Z](self, instance: "C[tuple[W, X, Y, Z]]", owner: type) -> "type[C[tuple[X, Y, Z, W]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T]:
    value: T
    __init__ = Initializer()

def check(cls: type[C[tuple[int, int, int, str]]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[tuple[int, int, int, str]]] = cls
    missing: Callable[[], C[tuple[int, int, int, str]]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[tuple[int, int, int, str]]] = cls  # error: [invalid-assignment]
```

## Specialized descriptors selecting finite constructor chains

In the example below, the initializer's type argument determines when the constructor chain ends.
`C[int]` progresses through `C[list[int]]` and `C[list[list[int]]]` before reaching `End`. Direct
calls and callback assignments retain the final initializer's required integer parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer[E]:
    @overload
    def __get__(self, instance: "C[E]", owner: type) -> type[End]: ...
    @overload
    def __get__[T](self, instance: "C[T]", owner: type) -> "type[C[list[T]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T]:
    value: T
    __init__ = Initializer[list[list[int]]]()

C[int](1)
C[int]()  # error: [missing-argument]
C[int]("x")  # error: [invalid-argument-type]

valid: Callable[[int], C[int]] = C[int]
missing: Callable[[], C[int]] = C[int]  # error: [invalid-assignment]
wrong: Callable[[str], C[int]] = C[int]  # error: [invalid-assignment]
```

## Constructor chains with changing descriptor specializations

In the example below, both type arguments grow on every step, but the first grows faster. Starting
with `C[int, list[list[int]]]` reaches two identical arguments after two steps, selecting `End`. The
descriptor's specialization changes with the second argument throughout this finite chain.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer[E]:
    @overload
    def __get__(self, instance: "C[E, E]", owner: type) -> type[End]: ...
    @overload
    def __get__[T, U](self, instance: "C[T, U]", owner: type) -> "type[C[list[list[T]], list[U]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T, U]:
    first: T
    second: U
    __init__ = Initializer[U]()

def check(cls: type[C[int, list[list[int]]]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[int, list[list[int]]]] = cls
    missing: Callable[[], C[int, list[list[int]]]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[int, list[list[int]]]] = cls  # error: [invalid-assignment]
```

Starting with `int` and `str` never produces identical arguments. Both direct calls and callback
assignments stop expanding that chain even though neither the constructor nor its descriptor repeats
an exact specialization.

```py
def growing(cls: type[C[int, str]]) -> None:
    cls()
    callback: Callable[..., C[int, str]] = cls
```

## Constructor chains introducing a new descriptor specialization

In the example below, the first step changes the descriptor's target from `str` to a nested list of
integers. Subsequent steps keep that target and grow the first argument until the two arguments
match. Callback signatures retain the integer required by the final initializer.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer[E]:
    @overload
    def __get__(self, instance: "C[E, E]", owner: type) -> type[End]: ...
    @overload
    def __get__[T, U](self, instance: "C[T, U]", owner: type) -> "type[C[list[T], list[list[list[int]]]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T, U]:
    first: T
    second: U
    __init__ = Initializer[U]()

def check(cls: type[C[int, str]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[int, str]] = cls
    missing: Callable[[], C[int, str]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[int, str]] = cls  # error: [invalid-assignment]
```

## Constructor descriptor overloads relating type arguments

In the example below, the terminating overload requires both constructor type arguments to be the
same type. Each argument can match `E` individually, but the overload matches only when their
inferred types agree. The first argument grows faster and catches up with the second after five
steps, so direct calls and callback signatures retain the integer required by `End`.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer:
    @overload
    def __get__[E](self, instance: "C[E, E]", owner: type) -> type[End]: ...
    @overload
    def __get__[T, U](self, instance: "C[T, U]", owner: type) -> "type[C[list[list[T]], list[U]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T, U]:
    first: T
    second: U
    __init__ = Initializer()

type Start = C[int, list[list[list[list[list[int]]]]]]

def check(cls: type[Start]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], Start] = cls
    missing: Callable[[], Start] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], Start] = cls  # error: [invalid-assignment]
```

## Constructor descriptor overloads matching through inheritance

In the example below, the initializer follows `C[int] -> C[Box[int]] -> C[Box[Box[int]]] -> End`.
The last receiver satisfies `Base[Base[Base[int]]]` through inheritance and covariance. Direct calls
and callback assignments retain the final initializer's required integer parameter.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Base[T]:
    def get(self) -> T:
        raise NotImplementedError

class Box[T](Base[T]): ...

class Initializer:
    @overload
    def __get__(self, instance: Base[Base[Base[int]]], owner: type) -> type[End]: ...
    @overload
    def __get__[T](self, instance: "C[T]", owner: type) -> "type[C[Box[T]]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[T](Base[T]):
    __init__ = Initializer()

def check(cls: type[C[int]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[int]] = cls
    missing: Callable[[], C[int]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[int]] = cls  # error: [invalid-assignment]
```

## Variadic constructor chains selected by descriptors

In the example below, each initializer appends an `int` to the type parameter tuple. The first
argument can be any type, including a nested list. Appending three integers reaches the terminating
overload, so the constructor requires an integer argument.

```py
from typing import Any, Callable, overload

class End:
    def __init__(self, value: int) -> None: ...

class Initializer:
    @overload
    def __get__[T](self, instance: "C[T, int, int, int]", owner: type) -> type[End]: ...
    @overload
    def __get__[*Ts](self, instance: "C[*Ts]", owner: type) -> "type[C[*Ts, int]]": ...
    def __get__(self, instance: Any, owner: type) -> Any: ...

class C[*Ts]:
    __init__ = Initializer()

def check(cls: type[C[list[list[list[int]]]]]) -> None:
    cls(1)
    cls()  # error: [missing-argument]
    cls("x")  # error: [invalid-argument-type]

    valid: Callable[[int], C[list[list[list[int]]]]] = cls
    missing: Callable[[], C[list[list[list[int]]]]] = cls  # error: [invalid-assignment]
    wrong: Callable[[str], C[list[list[list[int]]]]] = cls  # error: [invalid-assignment]
```

A `str` in the second position never matches that overload. Resolving the constructor stops
expanding the recursive chain for both a direct call and a callback assignment.

```py
def growing(cls: type[C[int, str]]) -> None:
    cls()
    callback: Callable[..., C[int, str]] = cls
```

## Generic `__iter__` methods with explicit receivers

Binding `__iter__` to an `Unpacker[Iterable[int]]` infers `S` as `int` from the explicit
`self: Unpacker[Iterable[S]]` annotation. Calls to `list()` and `iter()` preserve this element type,
just as `tuple()` and `for` loops do.

Regression test for <https://github.com/astral-sh/ty/issues/3598>.

```py
from collections.abc import Iterable, Iterator

class Unpacker[T: Iterable[object]]:
    def __init__(self, it: T, /) -> None:
        self._it = it
    def __iter__[S](self: "Unpacker[Iterable[S]]") -> Iterator[S]:
        return iter(self._it)

def integers() -> Unpacker[Iterable[int]]:
    return Unpacker([1, 2, 3])

reveal_type(tuple(integers()))  # revealed: tuple[int, ...]
for x in integers():
    reveal_type(x)  # revealed: int
reveal_type(list(integers()))  # revealed: list[int]
reveal_type(iter(integers()))  # revealed: Iterator[int]
```

## Naming a generic `Callable`: type aliases

The easiest way to refer to a generic `Callable` type directly is via a type alias:

```py
from typing import Callable
from ty_extensions._internal import generic_context

type IdentityCallable[T] = Callable[[T], T]

def decorator_factory[T]() -> IdentityCallable[T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions._internal.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
```

## Naming a generic `Callable` with paramspecs: type aliases

The same pattern holds if the callable involves a paramspec.

```py
from typing import Callable
from ty_extensions._internal import generic_context

type IdentityCallable[**P, T] = Callable[[Callable[P, T]], Callable[P, T]]

def decorator_factory[**P, T]() -> IdentityCallable[P, T]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions._internal.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return) -> T'return, /) -> ((**P'return) -> T'return)
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: [T](t: T) -> T
reveal_type(decorator_factory()(identity))
# revealed: Literal[1]
reveal_type(decorator_factory()(identity)(1))
```

## Naming a generic `Callable`: function return values

You can also return a generic `Callable` from a function. If a typevar _only_ appears inside of
`Callable`, and _only_ in return type position, then we treat the callable as generic, not the
function, just like above.

NOTE: This is one place where the PEP-695 syntax is misleading! It _looks_ like `decorator_factory`
is generic, since it contains a `[T]` binding context. However, we still notice that the only _use_
of `T` in the signature is in the return type, inside of a `Callable` — and so it is the returned
callable that is generic, not the function.

```py
from typing import Callable
from ty_extensions._internal import generic_context

def decorator_factory[T]() -> Callable[[T], T]:
    def decorator[T](fn: T) -> T:
        return fn
    # revealed: ty_extensions._internal.GenericContext[T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

# revealed: [T'return](T'return, /) -> T'return
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: Literal[1]
reveal_type(decorator_factory()(1))
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable[T](t: T) -> Callable[[T], T]:
    raise NotImplementedError

# revealed: ty_extensions._internal.GenericContext[T@outside_callable]
reveal_type(generic_context(outside_callable))

# revealed: (int, /) -> int
reveal_type(outside_callable(1))
# revealed: None
reveal_type(generic_context(outside_callable(1)))
# error: [invalid-argument-type]
outside_callable(1)("string")
```

## Naming a generic `Callable` with paramspecs: function return values

The same pattern holds if the callable involves a paramspec.

```py
from typing import Callable
from ty_extensions._internal import generic_context

def decorator_factory[**P, T]() -> Callable[[Callable[P, T]], Callable[P, T]]:
    def decorator[**P, T](fn: Callable[P, T]) -> Callable[P, T]:
        return fn
    # revealed: ty_extensions._internal.GenericContext[P@decorator, T@decorator]
    reveal_type(generic_context(decorator))

    return decorator

# Note that `decorator_factory` returns a generic callable, but is not itself generic!
# revealed: None
reveal_type(generic_context(decorator_factory))

def identity[T](t: T) -> T:
    return t

# revealed: [**P'return, T'return]((**P'return) -> T'return, /) -> ((**P'return) -> T'return)
reveal_type(decorator_factory())
# revealed: ty_extensions._internal.GenericContext[P'return@decorator_factory, T'return@decorator_factory]
reveal_type(generic_context(decorator_factory()))
# revealed: [T](t: T) -> T
reveal_type(decorator_factory()(identity))
# revealed: Literal[1]
reveal_type(decorator_factory()(identity)(1))
```

If the typevar also appears in a parameter, it is the function that is generic, and the returned
`Callable` is not:

```py
def outside_callable[**P, T](func: Callable[P, T]) -> Callable[P, T]:
    raise NotImplementedError

# revealed: ty_extensions._internal.GenericContext[P@outside_callable, T@outside_callable]
reveal_type(generic_context(outside_callable))

def int_identity(x: int) -> int:
    return x

# revealed: (x: int) -> int
reveal_type(outside_callable(int_identity))
# revealed: None
reveal_type(generic_context(outside_callable(int_identity)))
# error: [invalid-argument-type]
outside_callable(int_identity)("string")
```

The function's type parameters are still in scope inside the body, even if they only appear in a
return-position `Callable` and are scoped to the returned callable:

```py
from typing import Callable, cast

def body_annotation[**P]() -> Callable[P, None]:
    local: Callable[P, None] = cast(Callable[P, None], object())
    return local
```

## Inferring an explicit `object` upper bound from a callable

A type variable in a callable parameter position is constrained from above because callable
parameters are contravariant. An explicit `object` upper bound is still inference evidence; it is
different from having no inferred bound at all.

```py
from typing import Callable

def infer_from_consumer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_object(value: object) -> None: ...

reveal_type(infer_from_consumer(consume_object))  # revealed: object
```

## Intersecting inferred union upper bounds

Multiple callable arguments can infer multiple union upper bounds for the same type variable. We
keep those bounds factored and infer a compact type satisfying every bound rather than losing the
inference result while materializing their full cross product.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

def consume_left(value: A | B | C) -> None: ...
def consume_right(value: B | D | E) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: B
```

## Union without intersection does not consider budget

If the precise inferred solution comes from a single union type, rather than an intersection of
several unions, we return the precise solution.

```py
from typing import Callable, final

def infer_from_consumer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

def consume(value: A | B | C | D | E) -> None: ...

reveal_type(infer_from_consumer(consume))  # revealed: A | B | C | D | E
```

The same union remains precise when it is defined through nested type aliases:

```py
type FirstTwo = A | B
type NextTwo = C | D
type Options = FirstTwo | NextTwo | E

def consume_alias(value: Options) -> None: ...

reveal_type(infer_from_consumer(consume_alias))  # revealed: A | B | C | D | E
```

## Overlapping inferred union upper bounds with few surviving alternatives

The individual union upper bounds can exceed the solution budget when only a few alternatives
survive their intersection. Disjoint alternatives do not count toward the budget.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

@final
class F: ...

@final
class G: ...

@final
class H: ...

def consume_left(value: A | B | C | D | E) -> None: ...
def consume_right(value: A | B | F | G | H) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: A | B
```

Aliases for these unions also preserve the precise intersection in either argument order:

```py
type Left = A | B | C | D | E
type Right = A | B | F | G | H

def consume_left_alias(value: Left) -> None: ...
def consume_right_alias(value: Right) -> None: ...

reveal_type(infer_from_consumers(consume_left_alias, consume_right_alias))  # revealed: A | B
reveal_type(infer_from_consumers(consume_right_alias, consume_left_alias))  # revealed: A | B
```

## Intersecting aliased upper bounds exceeding the solution budget

Each consumer constrains `T` to a different union. The classes can share subclasses, so their
intersection has eight distinct alternatives. Type aliases do not exempt this expansion from the
solution budget: inference falls back to `Unknown` instead of constructing the entire intersection.

```py
from typing import Callable

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...

type First = A | B
type Second = C | D
type Third = E | F

def infer_from_consumers[T](
    first: Callable[[T], None],
    second: Callable[[T], None],
    third: Callable[[T], None],
) -> T:
    raise NotImplementedError

def consume_first(value: First) -> None: ...
def consume_second(value: Second) -> None: ...
def consume_third(value: Third) -> None: ...

reveal_type(infer_from_consumers(consume_first, consume_second, consume_third))  # revealed: Unknown
```

The same budget applies when an explicit union is intersected with aliased unions:

```py
def consume_explicit(value: E | F) -> None: ...

reveal_type(infer_from_consumers(consume_first, consume_second, consume_explicit))  # revealed: Unknown
```

## Narrowing negated intersection aliases

The negation of an aliased intersection acts as a union of negations. Intersecting three such upper
bounds can exceed the solution budget, but an additional literal upper bound leaves just one
solution. We infer the literal regardless of the consumer order.

```py
from typing import Callable, Literal
from ty_extensions import Intersection, Not

class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...

type AB = Intersection[A, B]
type CD = Intersection[C, D]
type EF = Intersection[E, F]

def infer_from_consumers[T](
    first: Callable[[T], None],
    second: Callable[[T], None],
    third: Callable[[T], None],
    fourth: Callable[[T], None],
) -> T:
    raise NotImplementedError

def exclude_ab(value: Not[AB]) -> None: ...
def exclude_cd(value: Not[CD]) -> None: ...
def exclude_ef(value: Not[EF]) -> None: ...
def consume_literal(value: Literal[5]) -> None: ...

reveal_type(infer_from_consumers(exclude_ab, exclude_cd, exclude_ef, consume_literal))  # revealed: Literal[5]
reveal_type(infer_from_consumers(consume_literal, exclude_ab, exclude_cd, exclude_ef))  # revealed: Literal[5]
```

## Intersecting recursive inferred union upper bounds

A recursive alias can contribute an upper bound without expanding its nested occurrences. Here, only
`int` satisfies both consumers, regardless of their order.

```py
from typing import Callable

type Recursive = int | list[Recursive]

def infer_from_consumers[T](left: Callable[[T], None], right: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_recursive(value: Recursive) -> None: ...
def consume_int_or_str(value: int | str) -> None: ...

reveal_type(infer_from_consumers(consume_recursive, consume_int_or_str))  # revealed: int
reveal_type(infer_from_consumers(consume_int_or_str, consume_recursive))  # revealed: int
```

## Narrowing recursive inferred union upper bounds

The third consumer restricts two recursive unions to the literal `5`. Its position does not change
the inferred result.

```py
from typing import Callable, Literal

class A: ...
class B: ...
class C: ...
class D: ...

type First = Literal[5] | A | B | list[First]
type Second = Literal[5] | C | D | list[Second]

def infer_from_consumers[T](
    first: Callable[[T], None],
    second: Callable[[T], None],
    third: Callable[[T], None],
) -> T:
    raise NotImplementedError

def consume_first(value: First) -> None: ...
def consume_second(value: Second) -> None: ...
def consume_literal(value: Literal[5]) -> None: ...

reveal_type(infer_from_consumers(consume_first, consume_second, consume_literal))  # revealed: Literal[5]
reveal_type(infer_from_consumers(consume_literal, consume_first, consume_second))  # revealed: Literal[5]
```

## Contextual generic return exceeding the solution budget

A generic call can receive an upper bound from the type context in which its return value is used.
An existing union in that upper bound should not consume the bounded-intersection budget unless an
intersection actually needs to be distributed over it.

```py
from collections.abc import Sequence
from typing import Literal

def make_list[T](value: T) -> list[T]:
    return [value]

def consume(values: Sequence[Literal["a", "b", "c", "d", "e"]] | None) -> None: ...

consume(make_list("a"))
```

## Disjoint inferred union upper bounds

If `Never` is the only type satisfying all inferred union upper bounds, it is the valid inferred
specialization for the type variable.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

def consume_left(value: A | B) -> None: ...
def consume_right(value: C | D) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: Never
```

## Disjoint inferred union upper bounds exceeding the solution budget

Large disjoint union upper bounds also exceed the budget before we can discover that their precise
intersection is bottom.

```py
from typing import Callable, final

def infer_from_consumers[T](
    left: Callable[[T], None],
    right: Callable[[T], None],
) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

@final
class C: ...

@final
class D: ...

@final
class E: ...

@final
class F: ...

@final
class G: ...

@final
class H: ...

@final
class I: ...

@final
class J: ...

def consume_left(value: A | B | C | D | E) -> None: ...
def consume_right(value: F | G | H | I | J) -> None: ...

reveal_type(infer_from_consumers(consume_left, consume_right))  # revealed: Never
```

## Combining inferred and declared upper bounds

A declared type-variable bound also participates when selecting a type that satisfies an inferred
union upper bound.

```py
from typing import Callable

def infer_str[T: str](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_int_or_str(value: int | str) -> None: ...

reveal_type(infer_str(consume_int_or_str))  # revealed: str
```

## Gradual class parameters

A callback that accepts `type[Any]` or `type[Unknown]` can accept any class object.

```py
from typing import Any, Callable
from ty_extensions._internal import Unknown

def invoke[T](callback: Callable[[type], T]) -> T:
    return callback(int)

def _(f: Callable[[type[Any]], int], g: Callable[[type[Unknown]], str]):
    reveal_type(invoke(f))  # revealed: int
    reveal_type(invoke(g))  # revealed: str
```

A gradual class argument can also satisfy a callback's metaclass parameter:

```py
class Meta(type): ...

def f(cls: Meta) -> int:
    return 1

def invoke_any[T](callback: Callable[[type[Any]], T], cls: type[Any]) -> T:
    return callback(cls)

def _(cls: type[Any]):
    reveal_type(invoke_any(f, cls))  # revealed: int
```

## Inferring gradual tuple returns with concrete bounds

A callback returning `tuple[Any, ...]` satisfies a fixed-length tuple bound because both its
elements and its length are gradual. Inference preserves the callback's return type.

```py
from typing import Any, Callable

def get_tuple() -> tuple[Any, ...]:
    return ()

def infer_fixed[T: tuple[int]](callback: Callable[[], T]) -> T:
    return callback()

reveal_type(infer_fixed(get_tuple))  # revealed: tuple[Any, ...]
```

The gradual length can also supply required elements at either end of a variable-length bound.

```py
def infer_prefix[T: tuple[int, *tuple[int, ...]]](callback: Callable[[], T]) -> T:
    return callback()

def infer_suffix[T: tuple[*tuple[int, ...], int]](callback: Callable[[], T]) -> T:
    return callback()

reveal_type(infer_prefix(get_tuple))  # revealed: tuple[Any, ...]
reveal_type(infer_suffix(get_tuple))  # revealed: tuple[Any, ...]
```

Fixed elements still have to satisfy the bound, and an ordinary homogeneous tuple does not have a
gradual length.

```py
def wrong_element() -> tuple[str, *tuple[Any, ...]]:
    return ("",)

def get_ints() -> tuple[int, ...]:
    return ()

infer_fixed(wrong_element)  # error: [invalid-argument-type]
infer_prefix(wrong_element)  # error: [invalid-argument-type]
infer_fixed(get_ints)  # error: [invalid-argument-type]
infer_prefix(get_ints)  # error: [invalid-argument-type]
infer_suffix(get_ints)  # error: [invalid-argument-type]
```

## Inferring type variables from gradual tuple elements

A callback returning a gradual-length tuple can constrain the type variables of a fixed-length
tuple.

```py
from typing import Any, Callable

def infer_pair[K, V](callback: Callable[[], tuple[K, V]]) -> tuple[K, V]:
    return callback()

def _(
    callback: Callable[[], tuple[Any, ...]],
    prefix: Callable[[], tuple[int, *tuple[Any, ...]]],
    suffix: Callable[[], tuple[*tuple[Any, ...], str]],
):
    reveal_type(infer_pair(callback))  # revealed: tuple[Any, Any]
    reveal_type(infer_pair(prefix))  # revealed: tuple[int, Any]
    reveal_type(infer_pair(suffix))  # revealed: tuple[Any, str]
```

A concrete homogeneous tuple does not guarantee the required length:

```py
def _(callback: Callable[[], tuple[int, ...]]):
    infer_pair(callback)  # error: [invalid-argument-type]
```

## Source type variables in gradual tuple returns

A callback's fixed tuple element can contain an outer type variable and still satisfy a concrete
bound. The gradual segment can be empty, and inference preserves the outer type variable.

```py
from typing import Any, Callable

def infer_tuple[R: tuple[object]](callback: Callable[[], R]) -> R:
    return callback()

def outer[T](callback: Callable[[], tuple[list[T], *tuple[Any, ...]]]) -> None:
    reveal_type(infer_tuple(callback))  # revealed: tuple[list[T@outer], *tuple[Any, ...]]
```

## Inferring `Never` from a callable parameter

`Never` is a valid upper-bound inference result and should not be replaced with the fallback for an
unsolved type variable.

```py
from typing import Callable, NoReturn

def infer_from_consumer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

def consume_never(value: NoReturn) -> None: ...

reveal_type(infer_from_consumer(consume_never))  # revealed: Never
```

## Conflicting inferred lower and upper bounds

A concrete argument can infer a lower bound that is incompatible with an upper bound inferred from a
callable argument. Such a call is invalid rather than producing a solution outside the inferred
upper bound.

```py
from typing import Callable, final

def infer_with_consumer[T](value: T, consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

@final
class A: ...

@final
class B: ...

def consume_b(value: B) -> None: ...

infer_with_consumer(A(), consume_b)  # error: [invalid-argument-type]
```

A callable argument can make a call invalid even when another argument satisfies the declared
type-variable bound. The diagnostic should describe the incompatible callable rather than claim that
`Base` violates its own bound.

```py
from typing import Callable

class Base: ...
class Input: ...

def call[T: Base](callback: Callable[[T], T], value: T) -> None:
    raise NotImplementedError

def callback(value: Input) -> Base:
    raise NotImplementedError

# error: [invalid-argument-type] "Argument to function `call` is incorrect: Expected `(Base, /) -> Base`, found `def callback(value: Input) -> Base`"
call(callback, Base())
```

## Combined upper bounds uses redundancy

When solving an upper bound involving a union, we should use the same typing relation to look for
redundant elements as we use for unions in general.

```py
from typing import Any, Callable, final

def infer[T](consumer: Callable[[T], None]) -> T:
    raise NotImplementedError

@final
class A: ...

def callback(value: A | Any) -> None: ...

reveal_type(infer(callback))  # revealed: A | Any
```

## Overloaded callable as generic `Callable` argument

An overloaded callable should be assignable to a non-overloaded callable type when the overload set
as a whole is compatible with the target callable.

The type variable should be inferred from the first matching overload, rather than unioning
parameter types across all overloads (which would create an unsatisfiable expected type for
contravariant type variables).

```py
from typing import Callable, overload

def accepts_callable[T](converter: Callable[[T], None]) -> T:
    raise NotImplementedError

@overload
def f(val: str) -> None: ...
@overload
def f(val: bytes) -> None: ...
def f(val: str | bytes) -> None:
    pass

reveal_type(accepts_callable(f))  # revealed: str | bytes
```

When overloads exchange their input and output types, the inferred return tuple currently contains a
union for each type variable.

```py
def infer_pair[T, U](converter: Callable[[T], U]) -> tuple[T, U]:
    raise NotImplementedError

@overload
def swap(value: int) -> str: ...
@overload
def swap(value: str) -> int: ...
def swap(value: int | str) -> int | str:
    raise NotImplementedError

# TODO: Infer the intersection of `tuple[int, str]` and `tuple[str, int]`.
# Both specializations validate the same call, so its result satisfies both return types.
reveal_type(infer_pair(swap))  # revealed: tuple[int | str, str | int]
```

When `T` is constrained to a union by other arguments, the overloaded callable must still be treated
as a whole to satisfy `Callable[[T], T]`.

```py
from typing import Callable, overload

def apply_twice[T](converter: Callable[[T], T], left: T, right: T) -> tuple[T, T]:
    return converter(left), converter(right)

@overload
def f(val: int) -> int: ...
@overload
def f(val: str) -> str: ...
def f(val: int | str) -> int | str:
    return val

x: int | str = 1
y: int | str = "a"

result = apply_twice(f, x, y)
# revealed: tuple[int | str, int | str]
reveal_type(result)
```

An overloaded callable returned from a generic callable factory should still be assignable to the
declared generic callable return type.

```py
from collections.abc import Callable, Coroutine
from typing import Any, overload

def singleton[S](flag: bool = False) -> Callable[[Callable[[int], S]], Callable[[int], S]]:
    @overload
    def wrapper[T](func: Callable[[int], Coroutine[Any, Any, T]]) -> Callable[[int], Coroutine[Any, Any, T]]: ...
    @overload
    def wrapper[U](func: Callable[[int], U]) -> Callable[[int], U]: ...
    def wrapper[T, U](func: Callable[[int], Coroutine[Any, Any, T] | U]) -> Callable[[int], Coroutine[Any, Any, T] | U]:
        return func

    return wrapper
```

## Multiple occurrences of a higher-order generic callable

If a generic callable is used more than once in a higher-order call, each occurrence should get its
own fresh typevars. In this example, the outer `partial` call receives a second, independent
occurrence of `partial` as its first argument, and `drop` as its second argument.

```py
from typing import Callable

def partial[A, B, C](c: Callable[[A, B], C], a: A) -> Callable[[B], C]:
    def inner(b: B) -> C:
        return c(a, b)
    return inner

def drop[X, Y](x: X, y: Y) -> Y:
    return y

# TODO: revealed: Literal["x"]
# We are correctly combining the constraint sets from both arguments of the outer
# `partial(partial, drop)` call: one from passing `partial` as `c`, and one from passing `drop` as
# `a`. However, we do that after having existentially quantified away the typevars from the generic
# `partial` when it's used as an argument, so this remains `Unknown` even after generic callable
# occurrences are freshened.
reveal_type(partial(partial, drop)(1)("x"))  # revealed: Unknown
# TODO: revealed: Literal[1]
reveal_type(partial(partial, drop)("x")(1))  # revealed: Unknown
```

## SymPy one-import MRE scaffold (multi-file)

Reduced regression lock for a SymPy overload/protocol shape that can panic in the
overload-assignability path.

```py
from __future__ import annotations

from sympy.polys.compatibility import Domain, IPolys
from typing import overload

class DefaultPrinting:
    pass

class PolyRing[T](DefaultPrinting, IPolys[T]):
    symbols: tuple[object, ...]
    domain: Domain[T]

    def clone(
        self,
        symbols: object | None = None,
        domain: object | None = None,
        order: object | None = None,
    ) -> PolyRing[T]:
        return self

    @overload
    def __getitem__(self, key: int) -> PolyRing[T]: ...
    @overload
    def __getitem__(self, key: slice) -> PolyRing[T] | Domain[T]: ...
    def __getitem__(self, key: slice | int) -> PolyRing[T] | Domain[T]:
        symbols = self.symbols[key]
        if not symbols:
            return self.domain
        return self.clone(symbols=symbols)

def takes_ring(x: PolyRing[int]) -> None:
    reveal_type(x[0])  # revealed: PolyRing[int]
    reveal_type(x[:])  # revealed: PolyRing[int] | Domain[int]
```

`sympy/polys/compatibility.pyi`:

```pyi
from __future__ import annotations

from typing import Protocol, overload

class Domain[T]: ...

class IPolys[T](Protocol):
    @overload
    def clone(
        self,
        symbols: object | None = None,
        domain: None = None,
        order: None = None,
    ) -> IPolys[T]: ...
    @overload
    def clone[S](
        self,
        symbols: object | None = None,
        *,
        domain: Domain[S],
        order: None = None,
    ) -> IPolys[S]: ...
    @overload
    def __getitem__(self, key: int) -> IPolys[T]: ...
    @overload
    def __getitem__(self, key: slice) -> IPolys[T] | Domain[T]: ...
```

## Returned callables with recursive parameter aliases

A type variable used by a recursive parameter alias belongs to the function. The returned callable
uses the type argument inferred from that parameter.

```py
from typing import Callable

type Tree[T] = tuple[T, Tree[T] | None]

def make[T](value: Tree[T]) -> Callable[[T], T]:
    raise NotImplementedError

callback = make((1, None))
reveal_type(callback)  # revealed: (int, /) -> int
callback("bad")  # error: [invalid-argument-type]
```

The type argument can also change at each recursive step.

```py
type Growing[T] = tuple[T, Growing[list[T]] | None]

def make_growing[T](value: Growing[T]) -> Callable[[T], T]:
    raise NotImplementedError

callback_growing = make_growing((1, None))
reveal_type(callback_growing)  # revealed: (int, /) -> int
callback_growing("bad")  # error: [invalid-argument-type]
```
