# Generic classes: Legacy syntax

## Defining a generic class

At its simplest, to define a generic class using the legacy syntax, you inherit from the
`typing.Generic` special form, which is "specialized" with the generic class's type variables.

```toml
[environment]
python-version = "3.11"
```

```py
from ty_extensions import generic_context
from typing_extensions import Generic, TypeVar, TypeVarTuple, ParamSpec, Unpack

T = TypeVar("T")
S = TypeVar("S")
P = ParamSpec("P")
Ts = TypeVarTuple("Ts")

class SingleTypevar(Generic[T]): ...
class MultipleTypevars(Generic[T, S]): ...
class SingleParamSpec(Generic[P]): ...
class TypeVarAndParamSpec(Generic[P, T]): ...
class SingleTypeVarTuple(Generic[Unpack[Ts]]): ...
class StarredSingleTypeVarTuple(Generic[*Ts]): ...
class TypeVarAndTypeVarTuple(Generic[T, Unpack[Ts]]): ...
class StarredTypeVarAndTypeVarTuple(Generic[T, *Ts]): ...

# revealed: ty_extensions.GenericContext[T@SingleTypevar]
reveal_type(generic_context(SingleTypevar))
# revealed: ty_extensions.GenericContext[T@MultipleTypevars, S@MultipleTypevars]
reveal_type(generic_context(MultipleTypevars))

# revealed: ty_extensions.GenericContext[P@SingleParamSpec]
reveal_type(generic_context(SingleParamSpec))
# revealed: ty_extensions.GenericContext[P@TypeVarAndParamSpec, T@TypeVarAndParamSpec]
reveal_type(generic_context(TypeVarAndParamSpec))

# TODO: support `TypeVarTuple` properly (these should not reveal `None`)
reveal_type(generic_context(SingleTypeVarTuple))  # revealed: None
reveal_type(generic_context(TypeVarAndTypeVarTuple))  # revealed: None
reveal_type(generic_context(StarredSingleTypeVarTuple))  # revealed: None
reveal_type(generic_context(StarredTypeVarAndTypeVarTuple))  # revealed: None
```

Inheriting from `Generic` multiple times yields a `duplicate-base` diagnostic, just like any other
class:

```py
class Bad(Generic[T], Generic[T]): ...  # error: [duplicate-base]
class AlsoBad(Generic[T], Generic[S]): ...  # error: [duplicate-base]
```

You cannot use the same typevar more than once.

```py
# TODO: error
class RepeatedTypevar(Generic[T, T]): ...
```

You can only specialize `typing.Generic` with typevars (TODO: or param specs or typevar tuples).

```py
# error: [invalid-argument-type] "`<class 'int'>` is not a valid argument to `Generic`"
class GenericOfType(Generic[int]): ...
```

You can also define a generic class by inheriting from some _other_ generic class, and specializing
it with typevars.

```py
class InheritedGeneric(MultipleTypevars[T, S]): ...
class InheritedGenericPartiallySpecialized(MultipleTypevars[T, int]): ...
class InheritedGenericFullySpecialized(MultipleTypevars[str, int]): ...

# revealed: ty_extensions.GenericContext[T@InheritedGeneric, S@InheritedGeneric]
reveal_type(generic_context(InheritedGeneric))
# revealed: ty_extensions.GenericContext[T@InheritedGenericPartiallySpecialized]
reveal_type(generic_context(InheritedGenericPartiallySpecialized))
# revealed: None
reveal_type(generic_context(InheritedGenericFullySpecialized))
```

In a nested class, references to typevars in an enclosing class are not allowed, but if they are
present, they are not included in the class's generic context.

```py
class OuterClass(Generic[T]):
    # error: [invalid-generic-class] "Generic class `InnerClass` must not reference type variables bound in an enclosing scope"
    class InnerClass(list[T]): ...
    # revealed: None
    reveal_type(generic_context(InnerClass))

    def method(self):
        # error: [invalid-generic-class] "Generic class `InnerClassInMethod` must not reference type variables bound in an enclosing scope"
        class InnerClassInMethod(list[T]): ...
        # revealed: None
        reveal_type(generic_context(InnerClassInMethod))

# revealed: ty_extensions.GenericContext[T@OuterClass]
reveal_type(generic_context(OuterClass))
```

If you don't specialize a generic base class, we use the default specialization, which maps each
typevar to its default value or `Any`. Since that base class is fully specialized, it does not make
the inheriting class generic.

```py
class InheritedGenericDefaultSpecialization(MultipleTypevars): ...

reveal_type(generic_context(InheritedGenericDefaultSpecialization))  # revealed: None
```

When inheriting from a generic class, you can optionally inherit from `typing.Generic` as well. But
if you do, you have to mention all of the typevars that you use in your other base classes.

```py
class ExplicitInheritedGeneric(MultipleTypevars[T, S], Generic[T, S]): ...

# error: [invalid-generic-class] "`Generic` base class must include all type variables used in other base classes"
class ExplicitInheritedGenericMissingTypevar(MultipleTypevars[T, S], Generic[T]): ...
class ExplicitInheritedGenericPartiallySpecialized(MultipleTypevars[T, int], Generic[T]): ...
class ExplicitInheritedGenericPartiallySpecializedExtraTypevar(MultipleTypevars[T, int], Generic[T, S]): ...

# error: [invalid-generic-class] "`Generic` base class must include all type variables used in other base classes"
class ExplicitInheritedGenericPartiallySpecializedMissingTypevar(MultipleTypevars[T, int], Generic[S]): ...

# revealed: ty_extensions.GenericContext[T@ExplicitInheritedGeneric, S@ExplicitInheritedGeneric]
reveal_type(generic_context(ExplicitInheritedGeneric))
# revealed: ty_extensions.GenericContext[T@ExplicitInheritedGenericPartiallySpecialized]
reveal_type(generic_context(ExplicitInheritedGenericPartiallySpecialized))
# revealed: ty_extensions.GenericContext[T@ExplicitInheritedGenericPartiallySpecializedExtraTypevar, S@ExplicitInheritedGenericPartiallySpecializedExtraTypevar]
reveal_type(generic_context(ExplicitInheritedGenericPartiallySpecializedExtraTypevar))
```

## Specializing generic classes explicitly

The type parameter can be specified explicitly:

```py
from typing_extensions import Generic, Literal, TypeVar

T = TypeVar("T")

class C(Generic[T]):
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
from typing import Union

BoundedT = TypeVar("BoundedT", bound=int)
BoundedByUnionT = TypeVar("BoundedByUnionT", bound=Union[int, str])

class Bounded(Generic[BoundedT]): ...
class BoundedByUnion(Generic[BoundedByUnionT]): ...
class IntSubclass(int): ...

reveal_type(Bounded[int]())  # revealed: Bounded[int]
reveal_type(Bounded[IntSubclass]())  # revealed: Bounded[IntSubclass]

# error: [invalid-type-arguments] "Type `str` is not assignable to upper bound `int` of type variable `BoundedT@Bounded`"
reveal_type(Bounded[str]())  # revealed: Bounded[Unknown]

# error:  [invalid-type-arguments] "Type `int | str` is not assignable to upper bound `int` of type variable `BoundedT@Bounded`"
reveal_type(Bounded[int | str]())  # revealed: Bounded[Unknown]

reveal_type(BoundedByUnion[int]())  # revealed: BoundedByUnion[int]
reveal_type(BoundedByUnion[IntSubclass]())  # revealed: BoundedByUnion[IntSubclass]
reveal_type(BoundedByUnion[str]())  # revealed: BoundedByUnion[str]
reveal_type(BoundedByUnion[int | str]())  # revealed: BoundedByUnion[int | str]
```

If the type variable is constrained, the specialized type must satisfy those constraints:

```py
ConstrainedT = TypeVar("ConstrainedT", int, str)

class Constrained(Generic[ConstrainedT]): ...

reveal_type(Constrained[int]())  # revealed: Constrained[int]

# TODO: error: [invalid-argument-type]
# TODO: revealed: Constrained[Unknown]
reveal_type(Constrained[IntSubclass]())  # revealed: Constrained[IntSubclass]

reveal_type(Constrained[str]())  # revealed: Constrained[str]

# TODO: error: [invalid-argument-type]
# TODO: revealed: Unknown
reveal_type(Constrained[int | str]())  # revealed: Constrained[int | str]

# error: [invalid-type-arguments] "Type `object` does not satisfy constraints `int`, `str` of type variable `ConstrainedT@Constrained`"
reveal_type(Constrained[object]())  # revealed: Constrained[Unknown]
```

If the type variable has a default, it can be omitted:

```py
WithDefaultU = TypeVar("WithDefaultU", default=int)

class WithDefault(Generic[T, WithDefaultU]): ...

reveal_type(WithDefault[str, str]())  # revealed: WithDefault[str, str]
reveal_type(WithDefault[str]())  # revealed: WithDefault[str, int]
```

## Diagnostics for bad specializations

We show the user where the type variable was defined if a specialization is given that doesn't
satisfy the type variable's upper bound or constraints:

<!-- snapshot-diagnostics -->

`library.py`:

```py
from typing import TypeVar, Generic

T = TypeVar("T", bound=str)
U = TypeVar("U", int, bytes)

class Bounded(Generic[T]):
    x: T

class Constrained(Generic[U]):
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
from typing_extensions import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    x: T

c: C[int] = C()
# TODO: revealed: C[int]
reveal_type(c)  # revealed: C[Unknown]
```

The typevars of a fully specialized generic class should no longer be visible:

```py
# TODO: revealed: int
reveal_type(c.x)  # revealed: Unknown
```

If the type parameter is not specified explicitly, and there are no constraints that let us infer a
specific type, we infer the typevar's default type:

```py
DefaultT = TypeVar("DefaultT", default=int)

class D(Generic[DefaultT]): ...

reveal_type(D())  # revealed: D[int]
```

If a typevar does not provide a default, we use `Unknown`:

```py
reveal_type(C())  # revealed: C[Unknown]
```

## Inferring generic class parameters from constructors

If the type of a constructor parameter is a class typevar, we can use that to infer the type
parameter. The types inferred from a type context and from a constructor parameter must be
consistent with each other.

### `__new__` only

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")

class C(Generic[T]):
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_callable(C)))

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[int | str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### `__init__` only

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")

class C(Generic[T]):
    def __init__(self, x: T) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_callable(C)))

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[int | str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Identical `__new__` and `__init__` signatures

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")

class C(Generic[T]):
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_callable(C)))

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[int | str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Compatible `__new__` and `__init__` signatures

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")

class C(Generic[T]):
    def __new__(cls, *args, **kwargs) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_callable(C)))

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[int | str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")

class D(Generic[T]):
    def __new__(cls, x: T) -> "D[T]":
        return object.__new__(cls)

    def __init__(self, *args, **kwargs) -> None: ...

# revealed: ty_extensions.GenericContext[T@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[T@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(D(1))  # revealed: D[int]

# error: [invalid-assignment] "Object of type `D[int | str]` is not assignable to `D[int]`"
wrong_innards: D[int] = D("five")
```

### Both present, `__new__` inherited from a generic base class

If either method comes from a generic base class, we don't currently use its inferred specialization
to specialize the class.

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")
U = TypeVar("U")
V = TypeVar("V")

class C(Generic[T, U]):
    def __new__(cls, *args, **kwargs) -> "C[T, U]":
        return object.__new__(cls)

class D(C[V, int]):
    def __init__(self, x: V) -> None: ...

# revealed: ty_extensions.GenericContext[V@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[V@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(D(1))  # revealed: D[int]
```

### Generic class inherits `__init__` from generic base class

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")
U = TypeVar("U")

class C(Generic[T, U]):
    def __init__(self, t: T, u: U) -> None: ...

class D(C[T, U]):
    pass

# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(C(1, "str"))  # revealed: C[int, str]
reveal_type(D(1, "str"))  # revealed: D[int, str]
```

### Generic class inherits `__init__` from `dict`

This is a specific example of the above, since it was reported specifically by a user.

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")
U = TypeVar("U")

class D(dict[T, U]):
    pass

# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(D(key=1))  # revealed: D[str, int]
```

### Generic class inherits `__new__` from `tuple`

(Technically, we synthesize a `__new__` method that is more precise than the one defined in typeshed
for `tuple`, so we use a different mechanism to make sure it has the right inherited generic
context. But from the user's point of view, this is another example of the above.)

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")
U = TypeVar("U")

class C(tuple[T, U]): ...

# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(into_callable(C)))

reveal_type(C((1, 2)))  # revealed: C[int, int]
```

### Upcasting a `tuple` to its `Sequence` supertype

This test is taken from the
[typing spec conformance suite](https://github.com/python/typing/blob/c141cdfb9d7085c1aafa76726c8ce08362837e8b/conformance/tests/tuples_type_compat.py#L133-L153)

```toml
[environment]
python-version = "3.11"
```

```py
from typing_extensions import TypeVar, Sequence, Never

T = TypeVar("T")

def test_seq(x: Sequence[T]) -> Sequence[T]:
    return x

def func8(t1: tuple[complex, list[int]], t2: tuple[int, *tuple[str, ...]], t3: tuple[()]):
    reveal_type(test_seq(t1))  # revealed: Sequence[int | float | complex | list[int]]
    reveal_type(test_seq(t2))  # revealed: Sequence[int | str]
    reveal_type(test_seq(t3))  # revealed: Sequence[Never]
```

### `__init__` is itself generic

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

S = TypeVar("S")
T = TypeVar("T")

class C(Generic[T]):
    def __init__(self, x: T, y: S) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C, S@__init__]
reveal_type(generic_context(into_callable(C)))

reveal_type(C(1, 1))  # revealed: C[int]
reveal_type(C(1, "string"))  # revealed: C[int]
reveal_type(C(1, True))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[int | str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five", 1)
```

### Some `__init__` overloads only apply to certain specializations

```py
from typing_extensions import overload, Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")
U = TypeVar("U")

class C(Generic[T]):
    @overload
    def __init__(self: "C[str]", x: str) -> None: ...
    @overload
    def __init__(self: "C[bytes]", x: bytes) -> None: ...
    @overload
    def __init__(self: "C[int]", x: bytes) -> None: ...
    @overload
    def __init__(self, x: int) -> None: ...
    def __init__(self, x: str | bytes | int) -> None: ...

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(into_callable(C)))

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

class D(Generic[T, U]):
    @overload
    def __init__(self: "D[str, U]", u: U) -> None: ...
    @overload
    def __init__(self, t: T, u: U) -> None: ...
    def __init__(self, *args) -> None: ...

# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(D("string"))  # revealed: D[str, str]
reveal_type(D(1))  # revealed: D[str, int]
reveal_type(D(1, "string"))  # revealed: D[int, str]
```

### Synthesized methods with dataclasses

```py
from dataclasses import dataclass
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")

@dataclass
class A(Generic[T]):
    x: T

# revealed: ty_extensions.GenericContext[T@A]
reveal_type(generic_context(A))
# revealed: ty_extensions.GenericContext[T@A]
reveal_type(generic_context(into_callable(A)))

reveal_type(A(x=1))  # revealed: A[int]
```

### Class typevar has another typevar as a default

```py
from typing_extensions import Generic, TypeVar
from ty_extensions import generic_context, into_callable

T = TypeVar("T")
U = TypeVar("U", default=T)

class C(Generic[T, U]): ...

# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(into_callable(C)))

reveal_type(C())  # revealed: C[Unknown, Unknown]

class D(Generic[T, U]):
    def __init__(self) -> None: ...

# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(D())  # revealed: D[Unknown, Unknown]
```

## Generic subclass

When a generic subclass fills its superclass's type parameter with one of its own, the actual types
propagate through:

```py
from typing_extensions import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U")
V = TypeVar("V")
W = TypeVar("W")

class Parent(Generic[T]):
    x: T

class ExplicitlyGenericChild(Parent[U], Generic[U]): ...
class ExplicitlyGenericGrandchild(ExplicitlyGenericChild[V], Generic[V]): ...
class ExplicitlyGenericGreatgrandchild(ExplicitlyGenericGrandchild[W], Generic[W]): ...
class ImplicitlyGenericChild(Parent[U]): ...
class ImplicitlyGenericGrandchild(ImplicitlyGenericChild[V]): ...
class ImplicitlyGenericGreatgrandchild(ImplicitlyGenericGrandchild[W]): ...

reveal_type(Parent[int]().x)  # revealed: int
reveal_type(ExplicitlyGenericChild[int]().x)  # revealed: int
reveal_type(ImplicitlyGenericChild[int]().x)  # revealed: int
reveal_type(ExplicitlyGenericGrandchild[int]().x)  # revealed: int
reveal_type(ImplicitlyGenericGrandchild[int]().x)  # revealed: int
reveal_type(ExplicitlyGenericGreatgrandchild[int]().x)  # revealed: int
reveal_type(ImplicitlyGenericGreatgrandchild[int]().x)  # revealed: int
```

## Generic methods

Generic classes can contain methods that are themselves generic. The generic methods can refer to
the typevars of the enclosing generic class, and introduce new (distinct) typevars that are only in
scope for the method.

```py
from ty_extensions import generic_context
from typing_extensions import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U")

class C(Generic[T]):
    def method(self, u: int) -> int:
        return u

    def generic_method(self, t: T, u: U) -> U:
        return u

# revealed: ty_extensions.GenericContext[T@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[Self@method]
reveal_type(generic_context(C.method))
# revealed: ty_extensions.GenericContext[Self@generic_method, U@generic_method]
reveal_type(generic_context(C.generic_method))
# revealed: None
reveal_type(generic_context(C[int]))
# revealed: ty_extensions.GenericContext[Self@method]
reveal_type(generic_context(C[int].method))
# revealed: ty_extensions.GenericContext[Self@generic_method, U@generic_method]
reveal_type(generic_context(C[int].generic_method))

c: C[int] = C[int]()
reveal_type(c.generic_method(1, "string"))  # revealed: Literal["string"]
# revealed: None
reveal_type(generic_context(c))
# revealed: ty_extensions.GenericContext[Self@method]
reveal_type(generic_context(c.method))
# revealed: ty_extensions.GenericContext[Self@generic_method, U@generic_method]
reveal_type(generic_context(c.generic_method))
```

## Specializations propagate

In a specialized generic alias, the specialization is applied to the attributes and methods of the
class.

```py
from typing_extensions import Generic, TypeVar, Protocol

T = TypeVar("T")
U = TypeVar("U")

class LinkedList(Generic[T]): ...

class C(Generic[T, U]):
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

class SomeProtocol(Protocol[T]):
    x: T

class Foo(Generic[T]):
    x: T

class D(Generic[T, U]):
    x: T
    y: U

    def method1(self) -> T:
        return self.x

    def method2(self) -> U:
        return self.y

    def method3(self) -> SomeProtocol[T]:
        return Foo()

d = D[int, str]()
reveal_type(d.x)  # revealed: int
reveal_type(d.y)  # revealed: str
reveal_type(d.method1())  # revealed: int
reveal_type(d.method2())  # revealed: str
reveal_type(d.method3())  # revealed: SomeProtocol[int]
reveal_type(d.method3().x)  # revealed: int
```

When a method is overloaded, the specialization is applied to all overloads.

```py
from typing_extensions import overload, Generic, TypeVar

S = TypeVar("S")

class WithOverloadedMethod(Generic[T]):
    @overload
    def method(self, x: T) -> T: ...
    @overload
    def method(self, x: S) -> S | T: ...
    def method(self, x: S | T) -> S | T:
        return x

# revealed: Overload[(self, x: int) -> int, (self, x: S@method) -> S@method | int]
reveal_type(WithOverloadedMethod[int].method)
```

## Cyclic class definitions

### F-bounded quantification

A class can use itself as the type parameter of one of its superclasses. (This is also known as the
[curiously recurring template pattern][crtp] or [F-bounded quantification][f-bound].)

#### In a stub file

Here, `Sub` is not a generic class, since it fills its superclass's type parameter (with itself).

```pyi
from typing_extensions import Generic, TypeVar

T = TypeVar("T")

class Base(Generic[T]): ...
class Sub(Base[Sub]): ...

reveal_type(Sub)  # revealed: <class 'Sub'>
```

#### With string forward references

A similar case can work in a non-stub file, if forward references are stringified:

```py
from typing_extensions import Generic, TypeVar

T = TypeVar("T")

class Base(Generic[T]): ...
class Sub(Base["Sub"]): ...

reveal_type(Sub)  # revealed: <class 'Sub'>

U = TypeVar("U")

class Base2(Generic[T, U]): ...
class Sub2(Base2["Sub2", U]): ...
```

#### Without string forward references

In a non-stub file, without stringified forward references, this raises a `NameError`:

```py
from typing_extensions import Generic, TypeVar

T = TypeVar("T")

class Base(Generic[T]): ...

# error: [unresolved-reference]
class Sub(Base[Sub]): ...
```

### Cyclic inheritance as a generic parameter

```pyi
from typing_extensions import Generic, TypeVar

T = TypeVar("T")

class Derived(list[Derived[T]], Generic[T]): ...
```

### Direct cyclic inheritance

Inheritance that would result in a cyclic MRO is detected as an error.

```py
from typing_extensions import Generic, TypeVar

T = TypeVar("T")

# error: [unresolved-reference]
class C(C, Generic[T]): ...

# error: [unresolved-reference]
class D(D[int], Generic[T]): ...
```

### Cyclic inheritance in a stub file combined with constrained type variables

This is a regression test for <https://github.com/astral-sh/ty/issues/1390>; we used to panic on
this:

`stub.pyi`:

```pyi
from typing import Generic, TypeVar

class A(B): ...
class G: ...

T = TypeVar("T", G, A)

class C(Generic[T]): ...
class B(C[A]): ...
class D(C[G]): ...

def func(x: D): ...

func(G())  # error: [invalid-argument-type]
```

[crtp]: https://en.wikipedia.org/wiki/Curiously_recurring_template_pattern
[f-bound]: https://en.wikipedia.org/wiki/Bounded_quantification#F-bounded_quantification
