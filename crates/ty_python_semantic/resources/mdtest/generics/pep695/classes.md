# Generic classes: PEP 695 syntax

```toml
[environment]
python-version = "3.13"
```

## Defining a generic class

At its simplest, to define a generic class using PEP 695 syntax, you add a list of `TypeVar`s,
`ParamSpec`s or `TypeVarTuple`s after the class name.

```py
from ty_extensions._internal import generic_context, reveal_mro

class SingleTypevar[T]: ...
class MultipleTypevars[T, S]: ...
class SingleParamSpec[**P]: ...
class TypeVarAndParamSpec[T, **P]: ...
class SingleTypeVarTuple[*Ts]: ...
class TypeVarAndTypeVarTuple[T, *Ts]: ...

# revealed: ty_extensions._internal.GenericContext[T@SingleTypevar]
reveal_type(generic_context(SingleTypevar))
# revealed: ty_extensions._internal.GenericContext[T@MultipleTypevars, S@MultipleTypevars]
reveal_type(generic_context(MultipleTypevars))

# revealed: ty_extensions._internal.GenericContext[P@SingleParamSpec]
reveal_type(generic_context(SingleParamSpec))
# revealed: ty_extensions._internal.GenericContext[T@TypeVarAndParamSpec, P@TypeVarAndParamSpec]
reveal_type(generic_context(TypeVarAndParamSpec))
# revealed: ty_extensions._internal.GenericContext[Ts@SingleTypeVarTuple]
reveal_type(generic_context(SingleTypeVarTuple))
# revealed: ty_extensions._internal.GenericContext[T@TypeVarAndTypeVarTuple, Ts@TypeVarAndTypeVarTuple]
reveal_type(generic_context(TypeVarAndTypeVarTuple))
```

Decorated generic classes still use the original class for their class-body generic context:

```py
class Wrap:
    def __init__(self, cls: type[object]) -> None: ...

@Wrap
class DecoratedGeneric[T]:
    value: T

    def get_value(self) -> T:
        return self.value

reveal_type(DecoratedGeneric)  # revealed: Wrap
```

You cannot use the same typevar more than once.

```py
# error: [invalid-syntax] "duplicate type parameter"
class RepeatedTypevar[T, T]: ...
```

You can also define a generic class by inheriting from some _other_ generic class, and specializing
it with typevars. With PEP 695 syntax, you must explicitly list all of the typevars that you use in
your base classes.

```py
class InheritedGeneric[U, V](MultipleTypevars[U, V]): ...
class InheritedGenericPartiallySpecialized[U](MultipleTypevars[U, int]): ...
class InheritedGenericFullySpecialized(MultipleTypevars[str, int]): ...

# revealed: ty_extensions._internal.GenericContext[U@InheritedGeneric, V@InheritedGeneric]
reveal_type(generic_context(InheritedGeneric))
# revealed: ty_extensions._internal.GenericContext[U@InheritedGenericPartiallySpecialized]
reveal_type(generic_context(InheritedGenericPartiallySpecialized))
# revealed: None
reveal_type(generic_context(InheritedGenericFullySpecialized))
```

If you don't specialize a generic base class, we use the default specialization, which maps each
typevar to its default value or `Any`. Since that base class is fully specialized, it does not make
the inheriting class generic.

```py
class InheritedGenericDefaultSpecialization(MultipleTypevars): ...  # error: [missing-type-argument]

# revealed: None
reveal_type(generic_context(InheritedGenericDefaultSpecialization))
```

You cannot use PEP-695 syntax and the legacy syntax in the same class definition.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

# error: [invalid-generic-class] "Cannot both inherit from `typing.Generic` and use PEP 695 type variables"
class BothGenericSyntaxes[U](Generic[T]): ...

reveal_mro(BothGenericSyntaxes)  # revealed: (<class 'BothGenericSyntaxes[Unknown]'>, Unknown, <class 'object'>)

# error: [invalid-generic-class] "Cannot both inherit from `typing.Generic` and use PEP 695 type variables"
# error: [invalid-base] "Cannot inherit from plain `Generic`"
class DoublyInvalid[T](Generic): ...

reveal_mro(DoublyInvalid)  # revealed: (<class 'DoublyInvalid[Unknown]'>, Unknown, <class 'object'>)
```

Legacy type variables also cannot be used to specialize another base class when the class uses PEP
695 syntax. A PEP 695 type parameter with the same name shadows the legacy type variable.

```py
K = TypeVar("K")

# error: [invalid-generic-class] "Legacy type variable `K` cannot be used in a PEP 695 class base"
class Bad[V](dict[K, V]): ...
class Good[K, V](dict[K, V]): ...
class Base[T]: ...

# TODO: error: [invalid-generic-class] "Legacy type variable `K` cannot be used in a PEP 695 class base"
class NormalizedBad[V](Base[K | object]): ...

class Methods[V]:
    def method(self, value: V, legacy: K) -> V | K:
        raise NotImplementedError
```

Generic classes implicitly inherit from `Generic`:

```py
class Foo[T]: ...

# revealed: (<class 'Foo[Unknown]'>, typing.Generic, <class 'object'>)
reveal_mro(Foo)
# revealed: (<class 'Foo[int]'>, typing.Generic, <class 'object'>)
reveal_mro(Foo[int])

class A: ...
class Bar[T](A): ...

# revealed: (<class 'Bar[Unknown]'>, <class 'A'>, typing.Generic, <class 'object'>)
reveal_mro(Bar)
# revealed: (<class 'Bar[int]'>, <class 'A'>, typing.Generic, <class 'object'>)
reveal_mro(Bar[int])

class B: ...
class Baz[T](A, B): ...

# revealed: (<class 'Baz[Unknown]'>, <class 'A'>, <class 'B'>, typing.Generic, <class 'object'>)
reveal_mro(Baz)
# revealed: (<class 'Baz[int]'>, <class 'A'>, <class 'B'>, typing.Generic, <class 'object'>)
reveal_mro(Baz[int])
```

## Class keyword arguments

Class keyword arguments are evaluated inside the type-parameter scope, so they must be resolved
cross-scope when validating against `__init_subclass__`:

```py
from typing import TypedDict

class Base:
    def __init_subclass__(cls, *, setting: int) -> None: ...

class Valid[T](Base, setting=1): ...
class InvalidType[T](Base, setting="x"): ...  # error: [invalid-argument-type]
class Fine[T](TypedDict, total=True): ...
class NotFine[T](TypedDict, total=None): ...  # error: [invalid-argument-type]

def _(kwargs: dict[str, int], bad_kwargs: dict[str, str]):
    class AlsoFine[T](Base, **kwargs): ...
    class AlsoNotFine[T](Base, **bad_kwargs): ...  # error: [invalid-argument-type]
```

## Specializing generic classes explicitly

The type parameter can be specified explicitly:

```py
from typing import Literal

class C[T]:
    x: T

reveal_type(C[int]())  # revealed: C[int]
reveal_type(C[Literal[5]]())  # revealed: C[Literal[5]]
```

The specialization must match the generic types:

```py
# error: [invalid-type-arguments] "Too many type arguments to class `C`: expected 1, got 2"
reveal_type(C[int, int]())  # revealed: C[Unknown]
```

If the type variable has an upper bound, the specialized type must satisfy that bound:

```py
class Bounded[T: int]: ...
class BoundedByUnion[T: int | str]: ...
class IntSubclass(int): ...

reveal_type(Bounded[int]())  # revealed: Bounded[int]
reveal_type(Bounded[IntSubclass]())  # revealed: Bounded[IntSubclass]

# error: [invalid-type-arguments] "Type `str` is not assignable to upper bound `int` of type variable `T@Bounded`"
reveal_type(Bounded[str]())  # revealed: Bounded[Unknown]

# error: [invalid-type-arguments] "Type `int | str` is not assignable to upper bound `int` of type variable `T@Bounded`"
reveal_type(Bounded[int | str]())  # revealed: Bounded[Unknown]

reveal_type(BoundedByUnion[int]())  # revealed: BoundedByUnion[int]
reveal_type(BoundedByUnion[IntSubclass]())  # revealed: BoundedByUnion[IntSubclass]
reveal_type(BoundedByUnion[str]())  # revealed: BoundedByUnion[str]
reveal_type(BoundedByUnion[int | str]())  # revealed: BoundedByUnion[int | str]
```

If the type variable is constrained, the specialized type must satisfy those constraints:

```py
class Constrained[T: (int, str)]: ...

reveal_type(Constrained[int]())  # revealed: Constrained[int]

# TODO: error: [invalid-argument-type]
# TODO: revealed: Constrained[Unknown]
reveal_type(Constrained[IntSubclass]())  # revealed: Constrained[IntSubclass]

reveal_type(Constrained[str]())  # revealed: Constrained[str]

# TODO: error: [invalid-argument-type]
# TODO: revealed: Unknown
reveal_type(Constrained[int | str]())  # revealed: Constrained[int | str]

# error: [invalid-type-arguments] "Type `object` does not satisfy constraints `int`, `str` of type variable `T@Constrained`"
reveal_type(Constrained[object]())  # revealed: Constrained[Unknown]
```

If the type variable has a default, it can be omitted:

```py
class WithDefault[T, U = int]: ...

reveal_type(WithDefault[str, str]())  # revealed: WithDefault[str, str]
reveal_type(WithDefault[str]())  # revealed: WithDefault[str, int]

# error: [invalid-type-arguments] "Too many type arguments to class `WithDefault`: expected between 1 and 2, got 3"
reveal_type(WithDefault[str, str, str]())  # revealed: WithDefault[Unknown, Unknown]
```

## Diagnostics for bad specializations

We show the user where the type variable was defined if a specialization is given that doesn't
satisfy the type variable's upper bound or constraints:

<!-- snapshot-diagnostics -->

`library.py`:

```py
class Bounded[T: str]:
    x: T

class Constrained[U: (int, bytes)]:
    x: U
```

`main.py`:

```py
from library import Bounded, Constrained

x: Bounded[int]  # error: [invalid-type-arguments]
y: Constrained[str]  # error: [invalid-type-arguments]
```

## Inferring generic class parameters

We can infer the type parameter from a type context:

```py
class C[T]:
    x: T

c: C[int] = C()
reveal_type(c)  # revealed: C[int]
```

The typevars of a fully specialized generic class should no longer be visible:

```py
reveal_type(c.x)  # revealed: int
```

If the type parameter is not specified explicitly, and there are no constraints that let us infer a
specific type, we infer the typevar's default type:

```py
class D[T = int]: ...

reveal_type(D())  # revealed: D[int]
```

If a typevar does not provide a default, we use `Unknown`:

```py
reveal_type(C())  # revealed: C[Unknown]
```

## Calls within the generic class

A call to a generic class from one of its own methods creates an independent generic occurrence. The
enclosing class's type variable does not constrain the new instance.

```py
class C[T]:
    def __init__(self) -> None: ...
    def method(self) -> None:
        reveal_type(C())  # revealed: C[Unknown]
        contextual: C[int] = C()
```

The same applies when an explicit `__new__` is followed by a downstream `__init__`. Both bound
receivers refer to the new generic occurrence.

```py
from typing import Self

class D[T]:
    def __new__(cls) -> Self:
        return super().__new__(cls)

    def __init__(self) -> None: ...
    def method(self) -> None:
        reveal_type(D())  # revealed: D[Unknown]
        contextual: D[int] = D()
```

## Inferring generic class parameters from constructors

If the type of a constructor parameter is a class typevar, we can use that to infer the type
parameter. The types inferred from a type context and from a constructor parameter must be
consistent with each other.

### `__new__` only

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T]:
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### `__init__` only

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T]:
    def __init__(self, x: T) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Failed constructor inference

A failed constructor call reports its argument error without exposing an unsolved class type
parameter or producing an additional assignment error.

```py
from collections.abc import Callable

class Animal: ...
class Dog(Animal): ...

class Consumer[T]:
    def __init__(self, callback: Callable[[T], None]) -> None:
        self.callback = callback

def accepts_dog(value: Dog) -> None: ...

consumer: Consumer[Animal] = Consumer(accepts_dog)  # error: [invalid-argument-type]
```

### Constructing the class from its own type variable

A constructor call inside a generic class can use a value whose type is one of the class's type
variables. The constructed instance keeps that type variable instead of falling back to `Unknown`,
so an incompatible type context is rejected.

```py
class C[T]:
    def __init__(self, value: T) -> None:
        reveal_type(C(value))  # revealed: C[T@C]

        # error: [invalid-assignment] "Object of type `C[T@C]` is not assignable to `C[int]`"
        invalid: C[int] = C(value)

    def from_union(self, value: T | list[T]) -> None:
        reveal_type(C(value))  # revealed: C[T@C | list[T@C]]

        # error: [invalid-assignment] "Object of type `C[T@C | list[T@C]]` is not assignable to `C[list[T@C]]`"
        invalid_union: C[list[T]] = C(value)
```

A method's own type variable is independent of the class type variable and is preserved in the same
way.

```py
class D[T]:
    def __init__(self, value: T) -> None: ...
    def method[S](self, value: S) -> None:
        reveal_type(D(value))  # revealed: D[S@method]
```

### Identical `__new__` and `__init__` signatures

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T]:
    x: T

    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Compatible `__new__` and `__init__` signatures

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T]:
    x: T

    def __new__(cls, *args, **kwargs) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C(1))  # revealed: C[int]

# TODO: The revealed type in the error message should be `C[str]`.
# error: [invalid-assignment] "Object of type `C[int | str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")

class D[T]:
    x: T

    def __new__(cls, x: T) -> "D[T]":
        return object.__new__(cls)

    def __init__(self, *args, **kwargs) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@D]
reveal_type(generic_context(D))
# revealed: ty_extensions._internal.GenericContext[T@D]
reveal_type(generic_context(into_regular_callable(D)))

reveal_type(D(1))  # revealed: D[int]

# TODO: The revealed type in the error message should be `D[str]`.
# error: [invalid-assignment] "Object of type `D[str | int]` is not assignable to `D[int]`"
wrong_innards: D[int] = D("five")
```

### Both present, `__new__` inherited from a generic base class

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T, U]:
    def __new__(cls, *args, **kwargs) -> "C[T, U]":
        return object.__new__(cls)

class D[V](C[V, int]):
    def __init__(self, x: V) -> None: ...

# revealed: ty_extensions._internal.GenericContext[V@D]
reveal_type(generic_context(D))
# revealed: ty_extensions._internal.GenericContext[V@D]
reveal_type(generic_context(into_regular_callable(D)))

# Because `C[T, U]` is not an instance of `D`, we never hit `D.__init__` at all.
reveal_type(D(1))  # revealed: C[Unknown, int]
```

### Explicit access to constructors of bare generic classes

Bare `__new__` and `__init__` members retain the class type variables in their generic contexts.
Each call can infer an owner specialization, while an explicit class specialization remains
authoritative.

```py
from typing import Self
from ty_extensions._internal import generic_context

class C[T = int]:
    def __new__(cls, value: T) -> Self:
        return super().__new__(cls)

    def __init__(self, value: T) -> None: ...

# revealed: ty_extensions._internal.GenericContext[Self@__new__, T@C]
reveal_type(generic_context(C.__new__))
# revealed: ty_extensions._internal.GenericContext[Self@__init__, T@C]
reveal_type(generic_context(C.__init__))

reveal_type(C.__new__(C[str], "value"))  # revealed: C[str]

def calls(c_str: C[str], c_int: C[int]) -> None:
    C.__init__(c_str, "value")
    C.__init__(c_int, 1)

    C[int].__init__(c_int, 1)

    # error: [invalid-argument-type]
    C[int].__init__(c_str, 1)
```

### Generic class inherits `__init__` from generic base class

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T, U]:
    def __init__(self, t: T, u: U) -> None: ...

class D[T, U](C[T, U]):
    pass

# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(into_regular_callable(D)))

reveal_type(C(1, "str"))  # revealed: C[Literal[1], Literal["str"]]
reveal_type(D(1, "str"))  # revealed: D[Literal[1], Literal["str"]]
```

### Generic class inherits `__init__` from `dict`

This is a specific example of the above, since it was reported specifically by a user.

```py
from ty_extensions._internal import generic_context, into_regular_callable

class D[T, U](dict[T, U]):
    pass

# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(into_regular_callable(D)))

reveal_type(D(key=1))  # revealed: D[str, int]
```

### Generic class inherits `__new__` from `tuple`

(Technically, we synthesize a `__new__` method that is more precise than the one defined in typeshed
for `tuple`, so we use a different mechanism to make sure it has the right inherited generic
context. But from the user's point of view, this is another example of the above.)

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T, U](tuple[T, U]): ...

# revealed: ty_extensions._internal.GenericContext[T@C, U@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C, U@C]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C((1, 2)))  # revealed: C[Literal[1], Literal[2]]
```

### Upcasting a `tuple` to its `Sequence` supertype

This test is taken from the
[typing spec conformance suite](https://github.com/python/typing/blob/c141cdfb9d7085c1aafa76726c8ce08362837e8b/conformance/tests/tuples_type_compat.py#L133-L153)

```py
from typing import Sequence, Never

def test_seq[T](x: Sequence[T]) -> Sequence[T]:
    return x

def func8(t1: tuple[complex, list[int]], t2: tuple[int, *tuple[str, ...]], t3: tuple[()]):
    reveal_type(test_seq(t1))  # revealed: Sequence[complex | list[int]]
    reveal_type(test_seq(t2))  # revealed: Sequence[int | str]
    reveal_type(test_seq(t3))  # revealed: Sequence[Never]
```

### `__init__` is itself generic

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T]:
    x: T

    def __init__[S](self, x: T, y: S) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C, S@__init__]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C(1, 1))  # revealed: C[int]
reveal_type(C(1, "string"))  # revealed: C[int]
reveal_type(C(1, True))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five", 1)
```

### Some `__init__` overloads only apply to certain specializations

```py
from __future__ import annotations
from typing import overload
from ty_extensions._internal import generic_context, into_regular_callable

class C[T]:
    @overload
    def __init__(self: C[str], x: str) -> None: ...
    @overload
    def __init__(self: C[bytes], x: bytes) -> None: ...
    @overload
    def __init__(self: C[int], x: bytes) -> None: ...
    @overload
    def __init__(self, x: int) -> None: ...
    def __init__(self, x: str | bytes | int) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C("string"))  # revealed: C[str]
reveal_type(C(b"bytes"))  # revealed: C[bytes]
reveal_type(C(12))  # revealed: C[Unknown]

C[str]("string")
C[str](b"bytes")  # error: [no-matching-overload]
C[str](12)

C[bytes]("string")  # error: [no-matching-overload]
C[bytes](b"bytes")
C[bytes](12)

C[int]("string")  # error: [no-matching-overload]
C[int](b"bytes")
C[int](12)

C[None]("string")  # error: [no-matching-overload]
C[None](b"bytes")  # error: [no-matching-overload]
C[None](12)

class D[T, U]:
    @overload
    def __init__(self: "D[str, U]", u: U) -> None: ...
    @overload
    def __init__(self, t: T, u: U) -> None: ...
    def __init__(self, *args) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(into_regular_callable(D)))

reveal_type(D("string"))  # revealed: D[str, Literal["string"]]
reveal_type(D(1))  # revealed: D[str, Literal[1]]
reveal_type(D(1, "string"))  # revealed: D[Literal[1], Literal["string"]]
```

### Synthesized methods with dataclasses

```py
from dataclasses import dataclass
from ty_extensions._internal import generic_context, into_regular_callable

@dataclass
class A[T]:
    x: T

# revealed: ty_extensions._internal.GenericContext[T@A]
reveal_type(generic_context(A))
# revealed: ty_extensions._internal.GenericContext[T@A]
reveal_type(generic_context(into_regular_callable(A)))

reveal_type(A(x=1))  # revealed: A[int]
```

### Class typevar has another typevar as a default

```py
from ty_extensions._internal import generic_context, into_regular_callable

class C[T, U = T]: ...

# revealed: ty_extensions._internal.GenericContext[T@C, U@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[T@C, U@C]
reveal_type(generic_context(into_regular_callable(C)))

reveal_type(C())  # revealed: C[Unknown, Unknown]

class D[T, U = T]:
    def __init__(self) -> None: ...

# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions._internal.GenericContext[T@D, U@D]
reveal_type(generic_context(into_regular_callable(D)))

reveal_type(D())  # revealed: D[Unknown, Unknown]
```

## Generic subclass

When a generic subclass fills its superclass's type parameter with one of its own, the actual types
propagate through:

```py
class Parent[T]:
    x: T

    @staticmethod
    def static(value: T) -> T:
        return value

class Child[U](Parent[U]): ...
class Grandchild[V](Child[V]): ...
class Greatgrandchild[W](Child[W]): ...

reveal_type(Parent[int]().x)  # revealed: int
reveal_type(Child[int]().x)  # revealed: int
reveal_type(Grandchild[int]().x)  # revealed: int
reveal_type(Greatgrandchild[int]().x)  # revealed: int
```

Attributes and static methods inherited by an unspecialized generic subclass use its default type
arguments instead of exposing its class-scoped type variables. Class access to generic instance
attributes is invalid, but the recovery types still use those defaults.

```py
# error: [invalid-attribute-access]
reveal_type(Parent.x)  # revealed: Unknown
# error: [invalid-attribute-access]
reveal_type(Child.x)  # revealed: Unknown
# error: [invalid-attribute-access]
reveal_type(Grandchild.x)  # revealed: Unknown

# revealed: def static(value: Unknown) -> Unknown
reveal_type(Child.static)
Child.static(1)
reveal_type(Child[int].static(1))  # revealed: int
```

Declared defaults must be preserved, and concrete arguments in partially specialized bases must not
be replaced with `Unknown`.

```py
class DefaultChild[T = int](Parent[T]): ...

class PairParent[T, U]:
    fixed: T
    unresolved: U

class PartiallyFixed[T](PairParent[int, T]): ...

# error: [invalid-attribute-access]
reveal_type(DefaultChild.x)  # revealed: int
# error: [invalid-attribute-access]
reveal_type(DefaultChild[str].x)  # revealed: str
reveal_type(PartiallyFixed.fixed)  # revealed: int
# error: [invalid-attribute-access]
reveal_type(PartiallyFixed.unresolved)  # revealed: Unknown
```

## Unbound inherited methods

An inherited method can be called through the subclass, passing the instance explicitly. Without
type arguments, `Child.get` uses `Unknown` for `U`; it does not infer `U` from the instance.

```py
class Parent[T]:
    def get(self) -> T:
        raise NotImplementedError

class Child[U](Parent[U]): ...

def _(child: Child[int]):
    reveal_type(Child.get(child))  # revealed: Unknown
```

An explicit default also determines which instances the method accepts. `DefaultChild.get` uses
`int` for `U`, so it accepts a `DefaultChild[int]` but rejects a `DefaultChild[str]`.

```py
class DefaultChild[U = int](Parent[U]): ...

def _(int_child: DefaultChild[int], str_child: DefaultChild[str]):
    reveal_type(DefaultChild.get(int_child))  # revealed: int
    DefaultChild.get(str_child)  # error: [invalid-argument-type]
```

Defaults also apply when a type parameter appears inside a base class's type argument. Only `U`
becomes `Unknown` below; the `int` in `tuple[int, U]` is unchanged.

```py
class NestedChild[U](Parent[tuple[int, U]]): ...

def _(child: NestedChild[str]):
    reveal_type(NestedChild.get(child))  # revealed: tuple[int, Unknown]
```

## Generic methods

Generic classes can contain methods that are themselves generic. The generic methods can refer to
the typevars of the enclosing generic class, and introduce new (distinct) typevars that are only in
scope for the method.

```py
from ty_extensions._internal import generic_context

class C[T]:
    def method(self, u: int) -> int:
        return u

    def generic_method[U](self, t: T, u: U) -> U:
        return u
    # error: [unresolved-reference]
    def cannot_use_outside_of_method(self, u: U): ...

    # error: [shadowed-type-variable]
    def cannot_shadow_class_typevar[T](self, t: T): ...

# revealed: ty_extensions._internal.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions._internal.GenericContext[Self@method]
reveal_type(generic_context(C.method))
# revealed: ty_extensions._internal.GenericContext[Self@generic_method, U@generic_method]
reveal_type(generic_context(C.generic_method))
# revealed: None
reveal_type(generic_context(C[int]))
# revealed: ty_extensions._internal.GenericContext[Self@method]
reveal_type(generic_context(C[int].method))
# revealed: ty_extensions._internal.GenericContext[Self@generic_method, U@generic_method]
reveal_type(generic_context(C[int].generic_method))

c: C[int] = C[int]()
reveal_type(c.generic_method(1, "string"))  # revealed: Literal["string"]
# revealed: None
reveal_type(generic_context(c))
# revealed: ty_extensions._internal.GenericContext[Self@method]
reveal_type(generic_context(c.method))
# revealed: ty_extensions._internal.GenericContext[Self@generic_method, U@generic_method]
reveal_type(generic_context(c.generic_method))
```

A class TypeVar remains fixed when a method is called from an enclosing generic function. The call
cannot specialize that enclosing occurrence merely because it also appears in synthetic `Self`'s
upper bound.

```py
class Container[T]:
    def replace(self, value: T) -> T:
        return value

def preserve[T](container: Container[T], value: T) -> T:
    return container.replace(value)

def cannot_choose_outer[T](container: Container[T]) -> T:
    # error: [invalid-argument-type]
    return container.replace(1)
```

## Generic instance attributes accessed through classes

Class access cannot select a specialization of an instance attribute. This restriction applies to
native type parameters just as it does to legacy `TypeVar` declarations.

```py
class Box[T]:
    value: T

# error: [invalid-attribute-access]
Box[int].value = 1
# error: [invalid-attribute-access]
Box[int].value
# error: [invalid-attribute-access]
Box.value = 1
# error: [invalid-attribute-access]
Box.value

box = Box[int]()
box.value = 1
reveal_type(box.value)  # revealed: Literal[1]
```

## Generic attributes accessed through subclass methods

The `cls` receiver in a classmethod, `__new__`, or `__init_subclass__` can refer to a concrete
subclass. We allow these methods to access generic attributes through their receiver.

```py
from typing import Self

class Box[T]:
    value: T

    @classmethod
    def get(cls) -> T:
        return cls.value

    def __new__(cls) -> Self:
        cls.value
        return super().__new__(cls)

    def __init_subclass__(cls, *, value: T) -> None:
        cls.value = value
        reveal_type(cls.value)  # revealed: T@Box

class Concrete(Box[int], value=1): ...

reveal_type(Concrete.get())  # revealed: int
```

## Generic attributes using type aliases

An alias can hide a dependency on a class type parameter, including inside a recursive alias. An
unused alias argument does not make the attribute depend on that type parameter.

```py
type Identity[T] = T
type Discard[T] = int
type Recursive[T] = T | list[Recursive[list[T]]]
type FixedRecursive = int | list[FixedRecursive]

class Box[T]:
    value: Identity[T]
    recursive: Recursive[T]
    constant: Discard[T]
    fixed_recursive: FixedRecursive

# error: [invalid-attribute-access]
Box[int].value
# error: [invalid-attribute-access]
Box[int].recursive
reveal_type(Box.constant)  # revealed: int
Box.fixed_recursive
```

Aliases can also contain a union of descriptor and non-descriptor types. Only the non-descriptor
alternatives are subject to the restriction on generic instance attributes.

```py
class Descriptor[T]:
    def __get__(self, instance: object, owner: type) -> int:
        return 0

type DescriptorOrList[T] = Descriptor[T] | list[T]
type Nested[T] = DescriptorOrList[T] | str
type DescriptorOrInt[T] = Descriptor[T] | int

class Aliased[T]:
    value: DescriptorOrList[T]
    nested: Nested[T]
    constant: DescriptorOrInt[T]

# error: [invalid-attribute-access]
Aliased[int].value = [1]
# error: [invalid-attribute-access]
reveal_type(Aliased[str].value)  # revealed: int | list[str]
# error: [invalid-attribute-access]
Aliased[int].nested
reveal_type(Aliased[str].constant)  # revealed: int
Aliased[int].constant = 1
```

## Specializations propagate

In a specialized generic alias, the specialization is applied to the attributes and methods of the
class.

```py
class LinkedList[T]: ...

class C[T, U]:
    x: T
    y: U

    def method1(self) -> T:
        return self.x

    def method2(self) -> U:
        return self.y

    def method3(self) -> LinkedList[T]:
        return LinkedList[T]()

c = C[int, str]()
reveal_type(c.x)  # revealed: int
reveal_type(c.y)  # revealed: str
reveal_type(c.method1())  # revealed: int
reveal_type(c.method2())  # revealed: str
reveal_type(c.method3())  # revealed: LinkedList[int]
```

When a method is overloaded, the specialization is applied to all overloads.

```py
from typing import overload

class WithOverloadedMethod[T]:
    @overload
    def method(self, x: T) -> T: ...
    @overload
    def method[S](self, x: S) -> S | T: ...
    def method[S](self, x: S | T) -> S | T:
        return x

# revealed: Overload[(self, x: int) -> int, [S](self, x: S) -> S | int]
reveal_type(WithOverloadedMethod[int].method)
```

## `Callable` return annotations preserve enclosing generic context

When a method annotation contains a `Callable[P, T]` return type, where `P`/`T` are bound by an
enclosing generic class or protocol, those typevars must remain tied to the enclosing context.

```py
from typing import Callable, Protocol, cast

class GenericClass[**P, T]:
    def hint(self) -> Callable[P, T]:
        raise NotImplementedError

class GenericProtocol[**P, T](Protocol):
    def hint(self) -> Callable[P, T]: ...

def class_case(x: GenericClass[[int], str]) -> None:
    # revealed: bound method GenericClass[(int, /), str].hint() -> ((int, /) -> str)
    reveal_type(x.hint)
    # revealed: (int, /) -> str
    reveal_type(x.hint())

def protocol_case(x: GenericProtocol[[int], str]) -> None:
    # revealed: bound method GenericProtocol[(int, /), str].hint() -> ((int, /) -> str)
    reveal_type(x.hint)
    # revealed: (int, /) -> str
    reveal_type(x.hint())
```

## Scoping of typevars

### No back-references

<!-- snapshot-diagnostics -->

Typevar bounds/constraints/defaults are lazy, but cannot refer to later typevars. Furthermore,
bounds/constraints cannot refer to other type variables, i.e. they must be non-generic.

```py
# error: [invalid-type-variable-bound]
class C[S: T, T]:
    pass

# error: [invalid-type-variable-bound]
class D[S, T: S]:
    pass

# error: [invalid-type-variable-constraints]
class E[S: (int, T), T]:
    pass

class F[S: X]:
    pass

X = int
```

Type variable defaults can reference earlier type variables, but not later ones:

```py
# This is fine: U's default references T, which comes before U
class Good[T, U = T]: ...

# error: [invalid-generic-class] "Default of `S` cannot reference later type parameter `T`"
class Bad[S = T, T = int]: ...

# error: [invalid-generic-class]
class AlsoBad[S = list[T], T = int]: ...
```

## Cyclic class definitions

### F-bounded quantification

A class can use itself as the type parameter of one of its superclasses. (This is also known as the
[curiously recurring template pattern][crtp] or [F-bounded quantification][f-bound].)

#### In a stub file

Here, `Sub` is not a generic class, since it fills its superclass's type parameter (with itself).

```pyi
class Base[T]: ...
class Sub(Base[Sub]): ...

reveal_type(Sub)  # revealed: <class 'Sub'>
```

#### With string forward references

A similar case can work in a non-stub file, if forward references are stringified:

```py
class Base[T]: ...
class Sub(Base["Sub"]): ...

reveal_type(Sub)  # revealed: <class 'Sub'>
```

#### Without string forward references

In a non-stub file, without stringified forward references, this raises a `NameError`:

```py
class Base[T]: ...

# error: [unresolved-reference]
class Sub(Base[Sub]): ...
```

### Cyclic inheritance as a generic parameter

```pyi
class Derived[T](list[Derived[T]]): ...
```

### Direct cyclic inheritance

Inheritance that would result in a cyclic MRO is detected as an error.

```pyi
# error: [cyclic-class-definition]
class C[T](C): ...

# error: [cyclic-class-definition]
class D[T](D[int]): ...
```

### Cyclic inheritance in a stub file combined with constrained type variables

This is a regression test for <https://github.com/astral-sh/ty/issues/1390>; we used to panic on
this:

`stub.pyi`:

```pyi
class A(B): ...
class G: ...
class C[T: (G, A)]: ...
class B(C[A]): ...
class D(C[G]): ...

def func(x: D): ...

func(G())  # error: [invalid-argument-type]
```

### Self-referential protocol with different specialization

This is a minimal reproduction for [ty#1874](https://github.com/astral-sh/ty/issues/1874).

```py
from __future__ import annotations
from typing import Protocol
from ty_extensions._internal import generic_context

class A[S, R](Protocol):
    def get(self, s: S) -> R: ...
    def set(self, s: S, r: R) -> S: ...
    def merge[R2](self, other: A[S, R2]) -> A[S, tuple[R, R2]]: ...

class Impl[S, R](A[S, R]):
    def foo(self, s: S) -> S:
        return self.set(s, self.get(s))

reveal_type(generic_context(A.get))  # revealed: ty_extensions._internal.GenericContext[Self@get]
reveal_type(generic_context(A.merge))  # revealed: ty_extensions._internal.GenericContext[Self@merge, R2@merge]
reveal_type(generic_context(Impl.foo))  # revealed: ty_extensions._internal.GenericContext[Self@foo]
```

## Subscripting non-generic classes

Subscripting a non-generic class in a type expression is an error. The invalid type expression
recovers to `Unknown`.

```py
class NonGeneric: ...

# error: [invalid-type-form] "Non-generic class `NonGeneric` cannot be specialized in a type expression"
def direct(value: NonGeneric[int]) -> None:
    reveal_type(value)  # revealed: Unknown
```

The same diagnostic applies when the specialization is nested inside `type[...]`.

```py
# error: [invalid-type-form] "Non-generic class `NonGeneric` cannot be specialized in a type expression"
def nested(value: type[NonGeneric[int]]) -> None:
    reveal_type(value)  # revealed: Unknown
```

Inheriting from a non-generic class, or from a specialization of a generic class, does not make the
subclass generic.

```py
class Child(NonGeneric): ...
class Generic[T, U = str]: ...
class SpecializedChild(Generic[int]): ...

# error: [invalid-type-form] "Non-generic class `Child` cannot be specialized in a type expression"
def child(value: Child[str]) -> None:
    reveal_type(value)  # revealed: Unknown

# error: [invalid-type-form] "Non-generic class `SpecializedChild` cannot be specialized in a type expression"
def specialized_child(value: SpecializedChild[bytes]) -> None:
    reveal_type(value)  # revealed: Unknown
```

## Custom class subscriptions in type expressions

Defining `__class_getitem__` makes a class subscriptable at runtime, but does not make it generic.
Its return type is used for value expressions, not for interpreting type expressions.

```py
class U:
    def __class_getitem__(cls, value: int) -> "type[U]":
        return U

reveal_type(U[0])  # revealed: type[U]
reveal_type(U.__class_getitem__(0))  # revealed: type[U]

# snapshot: invalid-type-form
def direct(value: U[0]) -> None:
    reveal_type(value)  # revealed: Unknown

# error: [invalid-type-form] "Non-generic class `U` cannot be specialized in a type expression"
def nested(value: type[U[0]]) -> None:
    reveal_type(value)  # revealed: Unknown
```

```snapshot
error[invalid-type-form]: Non-generic class `U` cannot be specialized in a type expression
 --> src/mdtest_snippet.py:9:19
  |
9 | def direct(value: U[0]) -> None:
  |                   ^^^^
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

## Custom class subscriptions with future annotations

Postponing annotation evaluation does not make a non-generic class a valid generic type. The error
concerns the type expression, not runtime subscription.

```py
from __future__ import annotations

class U:
    def __class_getitem__(cls, value: int) -> type[U]:
        return U

# error: [invalid-type-form]
def direct(value: U[0]) -> None:
    reveal_type(value)  # revealed: Unknown

# error: [invalid-type-form]
def nested(value: type[U[0]]) -> None:
    reveal_type(value)  # revealed: Unknown
```

## Custom class subscriptions with Python 3.14 annotations

The same type-expression error applies when Python defers annotation evaluation by default.

```toml
[environment]
python-version = "3.14"
```

```py
class U:
    def __class_getitem__(cls, value: int) -> type[U]:
        return U

# error: [invalid-type-form]
def direct(value: U[0]) -> None:
    reveal_type(value)  # revealed: Unknown

# error: [invalid-type-form]
def nested(value: type[U[0]]) -> None:
    reveal_type(value)  # revealed: Unknown
```

## Tuple as a PEP-695 generic class

Our special handling for `tuple` does not break if `tuple` is defined as a PEP-695 generic class in
typeshed:

```toml
[environment]
python-version = "3.12"
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class tuple[T]: ...
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

`main.py`:

```py
reveal_type((1, 2, 3))  # revealed: tuple[Literal[1], Literal[2], Literal[3]]
```

## Default type parameter after `TypeVarTuple`

<!-- snapshot-diagnostics -->

A type parameter with a default cannot follow a `TypeVarTuple` in a type parameter list. This is
prohibited by the typing spec because a `TypeVarTuple` consumes all remaining positional type
arguments, making any subsequent defaults meaningless.

```py
# error: [invalid-type-variable-default] "Type parameter `T` with a default follows TypeVarTuple `Ts`"
class Foo[*Ts, T = int]: ...

# error: [invalid-type-variable-default]
class Bar[T1, *Ts, T2 = int]: ...

# error: [invalid-type-variable-default]
class Baz[*Ts, T1 = int, T2 = str]: ...

# Note: the spec says this is fine,
# but it raises `TypeError` at runtime
# (<https://github.com/python/typing/issues/2211>)
#
# error: [invalid-type-variable-default]
class Qux[*Ts, **P = [int, str]]: ...

# error: [invalid-type-variable-default]
class Quux[*Ts, T1 = int, **P = [int, str]]: ...

# error: [invalid-type-variable-default]
class Corge[*Ts, T1 = int, T2 = str, **P = [int, str]]: ...

# error: [invalid-type-variable-default]
# error: [invalid-type-form] "Generic class `Grault` cannot have multiple `TypeVarTuple` type parameters"
class Grault[*Us, *Ts = *tuple[int, str]]: ...

# These are fine:
class Ok1[T, *Ts]: ...
class Ok3[*Ts]: ...
```

[crtp]: https://en.wikipedia.org/wiki/Curiously_recurring_template_pattern
[f-bound]: https://en.wikipedia.org/wiki/Bounded_quantification#F-bounded_quantification
