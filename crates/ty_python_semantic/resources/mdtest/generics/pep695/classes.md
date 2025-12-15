# Generic classes: PEP 695 syntax

```toml
[environment]
python-version = "3.13"
```

## Defining a generic class

At its simplest, to define a generic class using PEP 695 syntax, you add a list of `TypeVar`s,
`ParamSpec`s or `TypeVarTuple`s after the class name.

```py
from ty_extensions import generic_context, reveal_mro

class SingleTypevar[T]: ...
class MultipleTypevars[T, S]: ...
class SingleParamSpec[**P]: ...
class TypeVarAndParamSpec[T, **P]: ...
class SingleTypeVarTuple[*Ts]: ...
class TypeVarAndTypeVarTuple[T, *Ts]: ...

# revealed: ty_extensions.GenericContext[T@SingleTypevar]
reveal_type(generic_context(SingleTypevar))
# revealed: ty_extensions.GenericContext[T@MultipleTypevars, S@MultipleTypevars]
reveal_type(generic_context(MultipleTypevars))

# TODO: support `TypeVarTuple` properly
# (these should include the `TypeVarTuple`s in their generic contexts)
# revealed: ty_extensions.GenericContext[P@SingleParamSpec]
reveal_type(generic_context(SingleParamSpec))
# revealed: ty_extensions.GenericContext[T@TypeVarAndParamSpec, P@TypeVarAndParamSpec]
reveal_type(generic_context(TypeVarAndParamSpec))
# revealed: ty_extensions.GenericContext[]
reveal_type(generic_context(SingleTypeVarTuple))
# revealed: ty_extensions.GenericContext[T@TypeVarAndTypeVarTuple]
reveal_type(generic_context(TypeVarAndTypeVarTuple))
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

# revealed: ty_extensions.GenericContext[U@InheritedGeneric, V@InheritedGeneric]
reveal_type(generic_context(InheritedGeneric))
# revealed: ty_extensions.GenericContext[U@InheritedGenericPartiallySpecialized]
reveal_type(generic_context(InheritedGenericPartiallySpecialized))
# revealed: None
reveal_type(generic_context(InheritedGenericFullySpecialized))
```

If you don't specialize a generic base class, we use the default specialization, which maps each
typevar to its default value or `Any`. Since that base class is fully specialized, it does not make
the inheriting class generic.

```py
class InheritedGenericDefaultSpecialization(MultipleTypevars): ...

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
class D[T = int]: ...

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

We have to add `x: T` to the classes to ensure they're not bivariant in `T` (__new__ and __init__
signatures don't count towards variance).

### `__new__` only

```py
from ty_extensions import generic_context, into_callable

class C[T]:
    x: T

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
from ty_extensions import generic_context, into_callable

class C[T]:
    x: T

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
from ty_extensions import generic_context, into_callable

class C[T]:
    x: T

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
from ty_extensions import generic_context, into_callable

class C[T]:
    x: T

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

class D[T]:
    x: T

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
from ty_extensions import generic_context, into_callable

class C[T, U]:
    def __new__(cls, *args, **kwargs) -> "C[T, U]":
        return object.__new__(cls)

class D[V](C[V, int]):
    def __init__(self, x: V) -> None: ...

# revealed: ty_extensions.GenericContext[V@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[V@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(D(1))  # revealed: D[Literal[1]]
```

### Generic class inherits `__init__` from generic base class

```py
from ty_extensions import generic_context, into_callable

class C[T, U]:
    def __init__(self, t: T, u: U) -> None: ...

class D[T, U](C[T, U]):
    pass

# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(C(1, "str"))  # revealed: C[Literal[1], Literal["str"]]
reveal_type(D(1, "str"))  # revealed: D[Literal[1], Literal["str"]]
```

### Generic class inherits `__init__` from `dict`

This is a specific example of the above, since it was reported specifically by a user.

```py
from ty_extensions import generic_context, into_callable

class D[T, U](dict[T, U]):
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
from ty_extensions import generic_context, into_callable

class C[T, U](tuple[T, U]): ...

# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(into_callable(C)))

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
    reveal_type(test_seq(t1))  # revealed: Sequence[int | float | complex | list[int]]
    reveal_type(test_seq(t2))  # revealed: Sequence[int | str]
    reveal_type(test_seq(t3))  # revealed: Sequence[Never]
```

### `__init__` is itself generic

```py
from ty_extensions import generic_context, into_callable

class C[T]:
    x: T

    def __init__[S](self, x: T, y: S) -> None: ...

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
from __future__ import annotations
from typing import overload
from ty_extensions import generic_context, into_callable

class C[T]:
    # we need to use the type variable or else the class is bivariant in T, and
    # specializations become meaningless
    x: T

    @overload
    def __init__(self: C[str], x: str) -> None: ...
    @overload
    def __init__(self: C[bytes], x: bytes) -> None: ...
    @overload
    def __init__(self: C[int], x: bytes) -> None: ...
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

class D[T, U]:
    @overload
    def __init__(self: "D[str, U]", u: U) -> None: ...
    @overload
    def __init__(self, t: T, u: U) -> None: ...
    def __init__(self, *args) -> None: ...

# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(D))
# revealed: ty_extensions.GenericContext[T@D, U@D]
reveal_type(generic_context(into_callable(D)))

reveal_type(D("string"))  # revealed: D[str, Literal["string"]]
reveal_type(D(1))  # revealed: D[str, Literal[1]]
reveal_type(D(1, "string"))  # revealed: D[Literal[1], Literal["string"]]
```

### Synthesized methods with dataclasses

```py
from dataclasses import dataclass
from ty_extensions import generic_context, into_callable

@dataclass
class A[T]:
    x: T

# revealed: ty_extensions.GenericContext[T@A]
reveal_type(generic_context(A))
# revealed: ty_extensions.GenericContext[T@A]
reveal_type(generic_context(into_callable(A)))

reveal_type(A(x=1))  # revealed: A[int]
```

### Class typevar has another typevar as a default

```py
from ty_extensions import generic_context, into_callable

class C[T, U = T]: ...

# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(C))
# revealed: ty_extensions.GenericContext[T@C, U@C]
reveal_type(generic_context(into_callable(C)))

reveal_type(C())  # revealed: C[Unknown, Unknown]

class D[T, U = T]:
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
class Parent[T]:
    x: T

class Child[U](Parent[U]): ...
class Grandchild[V](Child[V]): ...
class Greatgrandchild[W](Child[W]): ...

reveal_type(Parent[int]().x)  # revealed: int
reveal_type(Child[int]().x)  # revealed: int
reveal_type(Grandchild[int]().x)  # revealed: int
reveal_type(Greatgrandchild[int]().x)  # revealed: int
```

## Generic methods

Generic classes can contain methods that are themselves generic. The generic methods can refer to
the typevars of the enclosing generic class, and introduce new (distinct) typevars that are only in
scope for the method.

```py
from ty_extensions import generic_context

class C[T]:
    def method(self, u: int) -> int:
        return u

    def generic_method[U](self, t: T, u: U) -> U:
        return u
    # error: [unresolved-reference]
    def cannot_use_outside_of_method(self, u: U): ...

    # TODO: error
    def cannot_shadow_class_typevar[T](self, t: T): ...

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

# revealed: Overload[(self, x: int) -> int, (self, x: S@method) -> S@method | int]
reveal_type(WithOverloadedMethod[int].method)
```

## Scoping of typevars

### No back-references

Typevar bounds/constraints/defaults are lazy, but cannot refer to later typevars:

```py
# TODO error
class C[S: T, T]:
    pass

class D[S: X]:
    pass

X = int
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
from ty_extensions import generic_context

class A[S, R](Protocol):
    def get(self, s: S) -> R: ...
    def set(self, s: S, r: R) -> S: ...
    def merge[R2](self, other: A[S, R2]) -> A[S, tuple[R, R2]]: ...

class Impl[S, R](A[S, R]):
    def foo(self, s: S) -> S:
        return self.set(s, self.get(s))

reveal_type(generic_context(A.get))  # revealed: ty_extensions.GenericContext[Self@get]
reveal_type(generic_context(A.merge))  # revealed: ty_extensions.GenericContext[Self@merge, R2@merge]
reveal_type(generic_context(Impl.foo))  # revealed: ty_extensions.GenericContext[Self@foo]
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

[crtp]: https://en.wikipedia.org/wiki/Curiously_recurring_template_pattern
[f-bound]: https://en.wikipedia.org/wiki/Bounded_quantification#F-bounded_quantification
