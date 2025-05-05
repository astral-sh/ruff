# Generic classes: Legacy syntax

## Defining a generic class

At its simplest, to define a generic class using the legacy syntax, you inherit from the
`typing.Generic` special form, which is "specialized" with the generic class's type variables.

```py
from ty_extensions import generic_context
from typing import Generic, TypeVar

T = TypeVar("T")
S = TypeVar("S")

class SingleTypevar(Generic[T]): ...
class MultipleTypevars(Generic[T, S]): ...

reveal_type(generic_context(SingleTypevar))  # revealed: tuple[T]
reveal_type(generic_context(MultipleTypevars))  # revealed: tuple[T, S]
```

You cannot use the same typevar more than once.

```py
# TODO: error
class RepeatedTypevar(Generic[T, T]): ...
```

You can only specialize `typing.Generic` with typevars (TODO: or param specs or typevar tuples).

```py
# error: [invalid-argument-type] "`Literal[int]` is not a valid argument to `typing.Generic`"
class GenericOfType(Generic[int]): ...
```

You can also define a generic class by inheriting from some _other_ generic class, and specializing
it with typevars.

```py
class InheritedGeneric(MultipleTypevars[T, S]): ...
class InheritedGenericPartiallySpecialized(MultipleTypevars[T, int]): ...
class InheritedGenericFullySpecialized(MultipleTypevars[str, int]): ...

reveal_type(generic_context(InheritedGeneric))  # revealed: tuple[T, S]
reveal_type(generic_context(InheritedGenericPartiallySpecialized))  # revealed: tuple[T]
reveal_type(generic_context(InheritedGenericFullySpecialized))  # revealed: None
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

reveal_type(generic_context(ExplicitInheritedGeneric))  # revealed: tuple[T, S]
reveal_type(generic_context(ExplicitInheritedGenericPartiallySpecialized))  # revealed: tuple[T]
reveal_type(generic_context(ExplicitInheritedGenericPartiallySpecializedExtraTypevar))  # revealed: tuple[T, S]
```

## Specializing generic classes explicitly

The type parameter can be specified explicitly:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
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
from typing import Union

BoundedT = TypeVar("BoundedT", bound=int)
BoundedByUnionT = TypeVar("BoundedByUnionT", bound=Union[int, str])

class Bounded(Generic[BoundedT]): ...
class BoundedByUnion(Generic[BoundedByUnionT]): ...
class IntSubclass(int): ...

reveal_type(Bounded[int]())  # revealed: Bounded[int]
reveal_type(Bounded[IntSubclass]())  # revealed: Bounded[IntSubclass]

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument to this function is incorrect: Expected `int`, found `str`"
reveal_type(Bounded[str]())  # revealed: Unknown

# TODO: update this diagnostic to talk about type parameters and specializations
# error:  [invalid-argument-type] "Argument to this function is incorrect: Expected `int`, found `int | str`"
reveal_type(Bounded[int | str]())  # revealed: Unknown

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

# TODO: update this diagnostic to talk about type parameters and specializations
# error: [invalid-argument-type] "Argument to this function is incorrect: Expected `int | str`, found `object`"
reveal_type(Constrained[object]())  # revealed: Unknown
```

If the type variable has a default, it can be omitted:

```py
WithDefaultU = TypeVar("WithDefaultU", default=int)

class WithDefault(Generic[T, WithDefaultU]): ...

reveal_type(WithDefault[str, str]())  # revealed: WithDefault[str, str]
reveal_type(WithDefault[str]())  # revealed: WithDefault[str, int]
```

## Inferring generic class parameters

We can infer the type parameter from a type context:

```py
from typing import Generic, TypeVar

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
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### `__init__` only

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Identical `__new__` and `__init__` signatures

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    def __new__(cls, x: T) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")
```

### Compatible `__new__` and `__init__` signatures

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    def __new__(cls, *args, **kwargs) -> "C[T]":
        return object.__new__(cls)

    def __init__(self, x: T) -> None: ...

reveal_type(C(1))  # revealed: C[Literal[1]]

# error: [invalid-assignment] "Object of type `C[Literal["five"]]` is not assignable to `C[int]`"
wrong_innards: C[int] = C("five")

class D(Generic[T]):
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
from typing import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U")
V = TypeVar("V")

class C(Generic[T, U]):
    def __new__(cls, *args, **kwargs) -> "C[T, U]":
        return object.__new__(cls)

class D(C[V, int]):
    def __init__(self, x: V) -> None: ...

reveal_type(D(1))  # revealed: D[Literal[1]]
```

### `__init__` is itself generic

```py
from typing import Generic, TypeVar

S = TypeVar("S")
T = TypeVar("T")

class C(Generic[T]):
    def __init__(self, x: T, y: S) -> None: ...

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
from typing import Generic, TypeVar

T = TypeVar("T")

class Base(Generic[T]):
    x: T | None = None

class ExplicitlyGenericSub(Base[T], Generic[T]): ...
class ImplicitlyGenericSub(Base[T]): ...

reveal_type(Base[int].x)  # revealed: int | None
reveal_type(ExplicitlyGenericSub[int].x)  # revealed: int | None
reveal_type(ImplicitlyGenericSub[int].x)  # revealed: int | None
```

## Generic methods

Generic classes can contain methods that are themselves generic. The generic methods can refer to
the typevars of the enclosing generic class, and introduce new (distinct) typevars that are only in
scope for the method.

```py
from typing import Generic, TypeVar

T = TypeVar("T")
U = TypeVar("U")

class C(Generic[T]):
    def method(self, u: U) -> U:
        return u

c: C[int] = C[int]()
reveal_type(c.method("string"))  # revealed: Literal["string"]
```

## Specializations propagate

In a specialized generic alias, the specialization is applied to the attributes and methods of the
class.

```py
from typing import Generic, TypeVar

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
```

## Cyclic class definitions

### F-bounded quantification

A class can use itself as the type parameter of one of its superclasses. (This is also known as the
[curiously recurring template pattern][crtp] or [F-bounded quantification][f-bound].)

#### In a stub file

Here, `Sub` is not a generic class, since it fills its superclass's type parameter (with itself).

```pyi
from typing import Generic, TypeVar

T = TypeVar("T")

class Base(Generic[T]): ...
class Sub(Base[Sub]): ...

reveal_type(Sub)  # revealed: Literal[Sub]
```

#### With string forward references

A similar case can work in a non-stub file, if forward references are stringified:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Base(Generic[T]): ...
class Sub(Base["Sub"]): ...

reveal_type(Sub)  # revealed: Literal[Sub]
```

#### Without string forward references

In a non-stub file, without stringified forward references, this raises a `NameError`:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Base(Generic[T]): ...

# error: [unresolved-reference]
class Sub(Base[Sub]): ...
```

### Cyclic inheritance as a generic parameter

```pyi
from typing import Generic, TypeVar

T = TypeVar("T")

class Derived(list[Derived[T]], Generic[T]): ...
```

### Direct cyclic inheritance

Inheritance that would result in a cyclic MRO is detected as an error.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

# error: [unresolved-reference]
class C(C, Generic[T]): ...

# error: [unresolved-reference]
class D(D[int], Generic[T]): ...
```

[crtp]: https://en.wikipedia.org/wiki/Curiously_recurring_template_pattern
[f-bound]: https://en.wikipedia.org/wiki/Bounded_quantification#F-bounded_quantification
