# Generic classes: PEP 695 syntax

```toml
[environment]
python-version = "3.13"
```

## Defining a generic class

At its simplest, to define a generic class using PEP 695 syntax, you add a list of `TypeVar`s,
`ParamSpec`s or `TypeVarTuple`s after the class name.

```py
from ty_extensions import generic_context

class SingleTypevar[T]: ...
class MultipleTypevars[T, S]: ...
class SingleParamSpec[**P]: ...
class TypeVarAndParamSpec[T, **P]: ...
class SingleTypeVarTuple[*Ts]: ...
class TypeVarAndTypeVarTuple[T, *Ts]: ...

# revealed: tuple[T@SingleTypevar]
reveal_type(generic_context(SingleTypevar))
# revealed: tuple[T@MultipleTypevars, S@MultipleTypevars]
reveal_type(generic_context(MultipleTypevars))

# TODO: support `ParamSpec`/`TypeVarTuple` properly
# (these should include the `ParamSpec`s and `TypeVarTuple`s in their generic contexts)
reveal_type(generic_context(SingleParamSpec))  # revealed: tuple[()]
reveal_type(generic_context(TypeVarAndParamSpec))  # revealed: tuple[T@TypeVarAndParamSpec]
reveal_type(generic_context(SingleTypeVarTuple))  # revealed: tuple[()]
reveal_type(generic_context(TypeVarAndTypeVarTuple))  # revealed: tuple[T@TypeVarAndTypeVarTuple]
```

You cannot use the same typevar more than once.

```py
# error: [invalid-syntax] "duplicate type parameter"
class RepeatedTypevar[T, T]: ...
```

You can only use typevars (TODO: or param specs or typevar tuples) in the class's generic context.

```py
# TODO: error
class GenericOfType[int]: ...
```

You can also define a generic class by inheriting from some _other_ generic class, and specializing
it with typevars. With PEP 695 syntax, you must explicitly list all of the typevars that you use in
your base classes.

```py
class InheritedGeneric[U, V](MultipleTypevars[U, V]): ...
class InheritedGenericPartiallySpecialized[U](MultipleTypevars[U, int]): ...
class InheritedGenericFullySpecialized(MultipleTypevars[str, int]): ...

# revealed: tuple[U@InheritedGeneric, V@InheritedGeneric]
reveal_type(generic_context(InheritedGeneric))
# revealed: tuple[U@InheritedGenericPartiallySpecialized]
reveal_type(generic_context(InheritedGenericPartiallySpecialized))
# revealed: None
reveal_type(generic_context(InheritedGenericFullySpecialized))
```

If you don't specialize a generic base class, we use the default specialization, which maps each
typevar to its default value or `Any`. Since that base class is fully specialized, it does not make
the inheriting class generic.

```py
class InheritedGenericDefaultSpecialization(MultipleTypevars): ...

reveal_type(generic_context(InheritedGenericDefaultSpecialization))  # revealed: None
```

You cannot use PEP-695 syntax and the legacy syntax in the same class definition.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

# error: [invalid-generic-class] "Cannot both inherit from `typing.Generic` and use PEP 695 type variables"
class BothGenericSyntaxes[U](Generic[T]): ...

reveal_type(BothGenericSyntaxes.__mro__)  # revealed: tuple[<class 'BothGenericSyntaxes[Unknown]'>, Unknown, <class 'object'>]

# error: [invalid-generic-class] "Cannot both inherit from `typing.Generic` and use PEP 695 type variables"
# error: [invalid-base] "Cannot inherit from plain `Generic`"
class DoublyInvalid[T](Generic): ...

reveal_type(DoublyInvalid.__mro__)  # revealed: tuple[<class 'DoublyInvalid[Unknown]'>, Unknown, <class 'object'>]
```

Generic classes implicitly inherit from `Generic`:

```py
class Foo[T]: ...

# revealed: tuple[<class 'Foo[Unknown]'>, typing.Generic, <class 'object'>]
reveal_type(Foo.__mro__)
# revealed: tuple[<class 'Foo[int]'>, typing.Generic, <class 'object'>]
reveal_type(Foo[int].__mro__)

class A: ...
class Bar[T](A): ...

# revealed: tuple[<class 'Bar[Unknown]'>, <class 'A'>, typing.Generic, <class 'object'>]
reveal_type(Bar.__mro__)
# revealed: tuple[<class 'Bar[int]'>, <class 'A'>, typing.Generic, <class 'object'>]
reveal_type(Bar[int].__mro__)

class B: ...
class Baz[T](A, B): ...

# revealed: tuple[<class 'Baz[Unknown]'>, <class 'A'>, <class 'B'>, typing.Generic, <class 'object'>]
reveal_type(Baz.__mro__)
# revealed: tuple[<class 'Baz[int]'>, <class 'A'>, <class 'B'>, typing.Generic, <class 'object'>]
reveal_type(Baz[int].__mro__)
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
# error: [too-many-positional-arguments] "Too many positional arguments to class `C`: expected 1, got 2"
reveal_type(C[int, int]())  # revealed: Unknown
```

If the type variable has an upper bound, the specialized type must satisfy that bound:

```py
class Bounded[T: int]: ...
class BoundedByUnion[T: int | str]: ...
class IntSubclass(int): ...

reveal_type(Bounded[int]())  # revealed: Bounded[int]
reveal_type(Bounded[IntSubclass]())  # revealed: Bounded[IntSubclass]

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument to class `Bounded` is incorrect: Expected `int`, found `str`"
reveal_type(Bounded[str]())  # revealed: Unknown

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument to class `Bounded` is incorrect: Expected `int`, found `int | str`"
reveal_type(Bounded[int | str]())  # revealed: Unknown

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

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument to class `Constrained` is incorrect: Expected `int | str`, found `object`"
reveal_type(Constrained[object]())  # revealed: Unknown
```

If the type variable has a default, it can be omitted:

```py
class WithDefault[T, U = int]: ...

reveal_type(WithDefault[str, str]())  # revealed: WithDefault[str, str]
reveal_type(WithDefault[str]())  # revealed: WithDefault[str, int]
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

### `__new__` only

```py
class C[T]:
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### `__init__` only

```py
class C[T]:
    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Identical `__new__` and `__init__` signatures

```py
class C[T]:
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Compatible `__new__` and `__init__` signatures

```py
class C[T]:
    def __new__(cls, *args, **kwargs) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")

class D[T]:
    def __new__(cls, x: T) -> "D[T]":
        return object.__new__(cls)

    def __init__(self, *args, **kwargs) -> None: ...

reveal_type(D(1))  # revealed: D[int]

# error: [invalid-assignment] "Object of type `D[str]` is not assignable to `D[int]`"
wrong_innards: D[int] = D("five")
```

### Both present, `__new__` inherited from a generic base class

If either method comes from a generic base class, we don't currently use its inferred specialization
to specialize the class.

```py
class C[T, U]:
    def __new__(cls, *args, **kwargs) -> "C[T, U]":
        return object.__new__(cls)

class D[V](C[V, int]):
    def __init__(self, x: V) -> None: ...

reveal_type(D(1))  # revealed: D[int]
```

### Generic class inherits `__init__` from generic base class

```py
class C[T, U]:
    def __init__(self, t: T, u: U) -> None: ...

class D[T, U](C[T, U]):
    pass

reveal_type(C(1, "str"))  # revealed: C[int, str]
reveal_type(D(1, "str"))  # revealed: D[int, str]
```

### Generic class inherits `__init__` from `dict`

This is a specific example of the above, since it was reported specifically by a user.

```py
class D[T, U](dict[T, U]):
    pass

reveal_type(D(key=1))  # revealed: D[str, int]
```

### Generic class inherits `__new__` from `tuple`

(Technically, we synthesize a `__new__` method that is more precise than the one defined in typeshed
for `tuple`, so we use a different mechanism to make sure it has the right inherited generic
context. But from the user's point of view, this is another example of the above.)

```py
class C[T, U](tuple[T, U]): ...

reveal_type(C((1, 2)))  # revealed: C[int, int]
```

### Upcasting a `tuple` to its `Sequence` supertype

This test is taken from the
[typing spec conformance suite](https://github.com/python/typing/blob/c141cdfb9d7085c1aafa76726c8ce08362837e8b/conformance/tests/tuples_type_compat.py#L133-L153)

```py
from typing import Sequence, Never

def test_seq[T](x: Sequence[T]) -> Sequence[T]:
    return x

def func8(t1: tuple[complex, list[int]], t2: tuple[int, *tuple[str, ...]], t3: tuple[()]):
    # TODO: should be `Sequence[int | float | complex | list[int]]`
    reveal_type(test_seq(t1))  # revealed: Sequence[Unknown]
    # TODO: should be `Sequence[int | str]`
    reveal_type(test_seq(t2))  # revealed: Sequence[Unknown]
    # TODO: this should be `Sequence[Never]`
    reveal_type(test_seq(t3))  # revealed: Sequence[Unknown]
```

### `__init__` is itself generic

```py
class C[T]:
    def __init__[S](self, x: T, y: S) -> None: ...

reveal_type(C(1, 1))  # revealed: C[int]
reveal_type(C(1, "string"))  # revealed: C[int]
reveal_type(C(1, True))  # revealed: C[int]

# error: [invalid-assignment] "Object of type `C[str]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five", 1)
```

### Some `__init__` overloads only apply to certain specializations

```py
from typing import overload

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
```

### Synthesized methods with dataclasses

```py
from dataclasses import dataclass

@dataclass
class A[T]:
    x: T

reveal_type(A(x=1))  # revealed: A[int]
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

reveal_type(generic_context(C))  # revealed: tuple[T@C]
reveal_type(generic_context(C.method))  # revealed: None
reveal_type(generic_context(C.generic_method))  # revealed: tuple[U@generic_method]
reveal_type(generic_context(C[int]))  # revealed: None
reveal_type(generic_context(C[int].method))  # revealed: None
reveal_type(generic_context(C[int].generic_method))  # revealed: tuple[U@generic_method]

c: C[int] = C[int]()
reveal_type(c.generic_method(1, "string"))  # revealed: Literal["string"]
reveal_type(generic_context(c))  # revealed: None
reveal_type(generic_context(c.method))  # revealed: None
reveal_type(generic_context(c.generic_method))  # revealed: tuple[U@generic_method]
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
    def method(self, x: T) -> T:
        return x

    @overload
    def method[S](self, x: S) -> S | T:
        return x

    def method[S](self, x: S | T) -> S | T:
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

```py
# error: [cyclic-class-definition]
class C[T](C): ...

# error: [cyclic-class-definition]
class D[T](D[int]): ...
```

[crtp]: https://en.wikipedia.org/wiki/Curiously_recurring_template_pattern
[f-bound]: https://en.wikipedia.org/wiki/Bounded_quantification#F-bounded_quantification
