# Generic classes: PEP 695 syntax

```toml
[environment]
python-version = "3.13"
```

## Defining a generic class

At its simplest, to define a generic class using PEP 695 syntax, you add a list of typevars after
the class name.

```py
from ty_extensions import generic_context

class SingleTypevar[T]: ...
class MultipleTypevars[T, S]: ...

reveal_type(generic_context(SingleTypevar))  # revealed: tuple[T]
reveal_type(generic_context(MultipleTypevars))  # revealed: tuple[T, S]
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

reveal_type(generic_context(InheritedGeneric))  # revealed: tuple[U, V]
reveal_type(generic_context(InheritedGenericPartiallySpecialized))  # revealed: tuple[U]
reveal_type(generic_context(InheritedGenericFullySpecialized))  # revealed: None
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

# error: [invalid-generic-class] "Cannot both inherit from `Generic` and use PEP 695 type variables"
class BothGenericSyntaxes[U](Generic[T]): ...
```

## Specializing generic classes explicitly

The type parameter can be specified explicitly:

```py
class C[T]:
    x: T

reveal_type(C[int]())  # revealed: C[int]
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
# error: [invalid-argument-type] "Argument to this function is incorrect: Expected `int`, found `str`"
reveal_type(Bounded[str]())  # revealed: Unknown

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument to this function is incorrect: Expected `int`, found `int | str`"
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
# error: [invalid-argument-type] "Argument to this function is incorrect: Expected `int | str`, found `object`"
reveal_type(Constrained[object]())  # revealed: Unknown
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

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### `__init__` only

```py
class C[T]:
    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Identical `__new__` and `__init__` signatures

```py
class C[T]:
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Compatible `__new__` and `__init__` signatures

```py
class C[T]:
    def __new__(cls, *args, **kwargs) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")

class D[T]:
    def __new__(cls, x: T) -> "D[T]":
        return object.__new__(cls)

    def __init__(self, *args, **kwargs) -> None: ...

reveal_type(D(1))  # revealed: D[Literal[1]]

# error: [invalid-assignment] "Object of type `D[Literal["five"]]` is not assignable to `D[int]`"
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

reveal_type(D(1))  # revealed: D[Literal[1]]
```

### `__init__` is itself generic

```py
class C[T]:
    def __init__[S](self, x: T, y: S) -> None: ...

reveal_type(C(1, 1))  # revealed: C[Literal[1]]
reveal_type(C(1, "string"))  # revealed: C[Literal[1]]
reveal_type(C(1, True))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five", 1)
```

## Generic subclass

When a generic subclass fills its superclass's type parameter with one of its own, the actual types
propagate through:

```py
class Base[T]:
    x: T | None = None

class Sub[U](Base[U]): ...

reveal_type(Base[int].x)  # revealed: int | None
reveal_type(Sub[int].x)  # revealed: int | None
```

## Generic methods

Generic classes can contain methods that are themselves generic. The generic methods can refer to
the typevars of the enclosing generic class, and introduce new (distinct) typevars that are only in
scope for the method.

```py
class C[T]:
    def method[U](self, u: U) -> U:
        return u
    # error: [unresolved-reference]
    def cannot_use_outside_of_method(self, u: U): ...

    # TODO: error
    def cannot_shadow_class_typevar[T](self, t: T): ...

c: C[int] = C[int]()
reveal_type(c.method("string"))  # revealed: Literal["string"]
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

reveal_type(Sub)  # revealed: Literal[Sub]
```

#### With string forward references

A similar case can work in a non-stub file, if forward references are stringified:

```py
class Base[T]: ...
class Sub(Base["Sub"]): ...

reveal_type(Sub)  # revealed: Literal[Sub]
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
