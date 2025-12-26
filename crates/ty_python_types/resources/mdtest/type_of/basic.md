# type special form

## Class literal

```py
class A: ...

def _(c: type[A]):
    reveal_type(c)  # revealed: type[A]
```

## Nested class literal

```py
class A:
    class B: ...

def f(c: type[A.B]):
    reveal_type(c)  # revealed: type[B]
```

## Deeply nested class literal

```py
class A:
    class B:
        class C: ...

def f(c: type[A.B.C]):
    reveal_type(c)  # revealed: type[C]
```

## Class literal from another module

```py
from a import A

def f(c: type[A]):
    reveal_type(c)  # revealed: type[A]
```

`a.py`:

```py
class A: ...
```

## Qualified class literal from another module

```py
import a

def f(c: type[a.B]):
    reveal_type(c)  # revealed: type[B]
```

`a.py`:

```py
class B: ...
```

## Deeply qualified class literal from another module

`a/test.py`:

```py
import a.b

def f(c: type[a.b.C]):
    reveal_type(c)  # revealed: type[C]
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

## New-style union of classes

```py
class BasicUser: ...
class ProUser: ...

class A:
    class B:
        class C: ...

def _(u: type[BasicUser | ProUser | A.B.C]):
    # revealed: type[BasicUser] | type[ProUser] | type[C]
    reveal_type(u)
```

## Old-style union of classes

```py
from typing import Union

class BasicUser: ...
class ProUser: ...

class A:
    class B:
        class C: ...

def f(a: type[Union[BasicUser, ProUser, A.B.C]], b: type[Union[str]], c: type[Union[BasicUser, Union[ProUser, A.B.C]]]):
    reveal_type(a)  # revealed: type[BasicUser] | type[ProUser] | type[C]
    reveal_type(b)  # revealed: type[str]
    reveal_type(c)  # revealed: type[BasicUser] | type[ProUser] | type[C]
```

## New-style and old-style unions in combination

```py
from typing import Union

class BasicUser: ...
class ProUser: ...

class A:
    class B:
        class C: ...

def f(a: type[BasicUser | Union[ProUser, A.B.C]], b: type[Union[BasicUser | Union[ProUser, A.B.C | str]]]):
    reveal_type(a)  # revealed: type[BasicUser] | type[ProUser] | type[C]
    reveal_type(b)  # revealed: type[BasicUser] | type[ProUser] | type[C] | type[str]
```

## Illegal parameters

```py
class A: ...
class B: ...

# error: [invalid-type-form]
_: type[A, B]
```

## As a base class

```py
from ty_extensions import reveal_mro

class Foo(type[int]): ...

reveal_mro(Foo)  # revealed: (<class 'Foo'>, <class 'type'>, <class 'object'>)
```

## `@final` classes

`type[]` types are eagerly converted to class-literal types if a class decorated with `@final` is
used as the type argument. This applies to standard-library classes and user-defined classes. The
same also applies to enum classes with members, which are implicitly final:

```toml
[environment]
python-version = "3.12"
```

```py
from types import EllipsisType
from typing import final
from enum import Enum

@final
class Foo: ...

class Answer(Enum):
    NO = 0
    YES = 1

def _(x: type[Foo], y: type[EllipsisType], z: type[Answer]):
    reveal_type(x)  # revealed: <class 'Foo'>
    reveal_type(y)  # revealed: <class 'EllipsisType'>
    reveal_type(z)  # revealed: <class 'Answer'>
```

## Subtyping `@final` classes

```toml
[environment]
python-version = "3.12"
```

```py
from typing import final, Any
from ty_extensions import is_assignable_to, is_subtype_of, is_disjoint_from, static_assert

class Biv[T]: ...

class Cov[T]:
    def pop(self) -> T:
        raise NotImplementedError

class Contra[T]:
    def push(self, value: T) -> None:
        pass

class Inv[T]:
    x: T

@final
class BivSub[T](Biv[T]): ...

@final
class CovSub[T](Cov[T]): ...

@final
class ContraSub[T](Contra[T]): ...

@final
class InvSub[T](Inv[T]): ...

def _[T, U]():
    static_assert(is_subtype_of(type[BivSub[T]], type[BivSub[U]]))
    static_assert(not is_disjoint_from(type[BivSub[U]], type[BivSub[T]]))

    # `T` and `U` could specialize to the same type.
    static_assert(not is_subtype_of(type[CovSub[T]], type[CovSub[U]]))
    static_assert(not is_disjoint_from(type[CovSub[U]], type[CovSub[T]]))

    static_assert(not is_subtype_of(type[ContraSub[T]], type[ContraSub[U]]))
    static_assert(not is_disjoint_from(type[ContraSub[U]], type[ContraSub[T]]))

    static_assert(not is_subtype_of(type[InvSub[T]], type[InvSub[U]]))
    static_assert(not is_disjoint_from(type[InvSub[U]], type[InvSub[T]]))

def _():
    static_assert(is_subtype_of(type[BivSub[bool]], type[BivSub[int]]))
    static_assert(is_subtype_of(type[BivSub[int]], type[BivSub[bool]]))
    static_assert(not is_disjoint_from(type[BivSub[bool]], type[BivSub[int]]))
    # `BivSub[int]` and `BivSub[str]` are mutual subtypes.
    static_assert(not is_disjoint_from(type[BivSub[int]], type[BivSub[str]]))

    static_assert(is_subtype_of(type[CovSub[bool]], type[CovSub[int]]))
    static_assert(not is_subtype_of(type[CovSub[int]], type[CovSub[bool]]))
    static_assert(not is_disjoint_from(type[CovSub[bool]], type[CovSub[int]]))
    # `CovSub[Never]` is a subtype of both `CovSub[int]` and `CovSub[str]`.
    static_assert(not is_disjoint_from(type[CovSub[int]], type[CovSub[str]]))

    static_assert(not is_subtype_of(type[ContraSub[bool]], type[ContraSub[int]]))
    static_assert(is_subtype_of(type[ContraSub[int]], type[ContraSub[bool]]))
    static_assert(not is_disjoint_from(type[ContraSub[bool]], type[ContraSub[int]]))
    # `ContraSub[int | str]` is a subtype of both `ContraSub[int]` and `ContraSub[str]`.
    static_assert(not is_disjoint_from(type[ContraSub[int]], type[ContraSub[str]]))

    static_assert(not is_subtype_of(type[InvSub[bool]], type[InvSub[int]]))
    static_assert(not is_subtype_of(type[InvSub[int]], type[InvSub[bool]]))
    static_assert(is_disjoint_from(type[InvSub[int]], type[InvSub[str]]))
    # TODO: These are disjoint.
    static_assert(not is_disjoint_from(type[InvSub[bool]], type[InvSub[int]]))

def _[T]():
    static_assert(is_subtype_of(type[BivSub[T]], type[BivSub[Any]]))
    static_assert(is_subtype_of(type[BivSub[Any]], type[BivSub[T]]))
    static_assert(is_assignable_to(type[BivSub[T]], type[BivSub[Any]]))
    static_assert(is_assignable_to(type[BivSub[Any]], type[BivSub[T]]))
    static_assert(not is_disjoint_from(type[BivSub[T]], type[BivSub[Any]]))

    static_assert(not is_subtype_of(type[CovSub[T]], type[CovSub[Any]]))
    static_assert(not is_subtype_of(type[CovSub[Any]], type[CovSub[T]]))
    static_assert(is_assignable_to(type[CovSub[T]], type[CovSub[Any]]))
    static_assert(is_assignable_to(type[CovSub[Any]], type[CovSub[T]]))
    static_assert(not is_disjoint_from(type[CovSub[T]], type[CovSub[Any]]))

    static_assert(not is_subtype_of(type[ContraSub[T]], type[ContraSub[Any]]))
    static_assert(not is_subtype_of(type[ContraSub[Any]], type[ContraSub[T]]))
    static_assert(is_assignable_to(type[ContraSub[T]], type[ContraSub[Any]]))
    static_assert(is_assignable_to(type[ContraSub[Any]], type[ContraSub[T]]))
    static_assert(not is_disjoint_from(type[ContraSub[T]], type[ContraSub[Any]]))

    static_assert(not is_subtype_of(type[InvSub[T]], type[InvSub[Any]]))
    static_assert(not is_subtype_of(type[InvSub[Any]], type[InvSub[T]]))
    static_assert(is_assignable_to(type[InvSub[T]], type[InvSub[Any]]))
    static_assert(is_assignable_to(type[InvSub[Any]], type[InvSub[T]]))
    static_assert(not is_disjoint_from(type[InvSub[T]], type[InvSub[Any]]))

def _[T, U]():
    static_assert(is_subtype_of(type[BivSub[T]], type[Biv[T]]))
    static_assert(not is_subtype_of(type[Biv[T]], type[BivSub[T]]))
    static_assert(not is_disjoint_from(type[BivSub[T]], type[Biv[T]]))
    static_assert(not is_disjoint_from(type[BivSub[U]], type[Biv[T]]))
    static_assert(not is_disjoint_from(type[BivSub[U]], type[Biv[U]]))

    static_assert(is_subtype_of(type[CovSub[T]], type[Cov[T]]))
    static_assert(not is_subtype_of(type[Cov[T]], type[CovSub[T]]))
    static_assert(not is_disjoint_from(type[CovSub[T]], type[Cov[T]]))
    static_assert(not is_disjoint_from(type[CovSub[U]], type[Cov[T]]))
    static_assert(not is_disjoint_from(type[CovSub[U]], type[Cov[U]]))

    static_assert(is_subtype_of(type[ContraSub[T]], type[Contra[T]]))
    static_assert(not is_subtype_of(type[Contra[T]], type[ContraSub[T]]))
    static_assert(not is_disjoint_from(type[ContraSub[T]], type[Contra[T]]))
    static_assert(not is_disjoint_from(type[ContraSub[U]], type[Contra[T]]))
    static_assert(not is_disjoint_from(type[ContraSub[U]], type[Contra[U]]))

    static_assert(is_subtype_of(type[InvSub[T]], type[Inv[T]]))
    static_assert(not is_subtype_of(type[Inv[T]], type[InvSub[T]]))
    static_assert(not is_disjoint_from(type[InvSub[T]], type[Inv[T]]))
    static_assert(not is_disjoint_from(type[InvSub[U]], type[Inv[T]]))
    static_assert(not is_disjoint_from(type[InvSub[U]], type[Inv[U]]))

def _():
    static_assert(is_subtype_of(type[BivSub[bool]], type[Biv[int]]))
    static_assert(is_subtype_of(type[BivSub[int]], type[Biv[bool]]))
    static_assert(not is_disjoint_from(type[BivSub[bool]], type[Biv[int]]))
    static_assert(not is_disjoint_from(type[BivSub[int]], type[Biv[bool]]))

    static_assert(is_subtype_of(type[CovSub[bool]], type[Cov[int]]))
    static_assert(not is_subtype_of(type[CovSub[int]], type[Cov[bool]]))
    static_assert(not is_disjoint_from(type[CovSub[bool]], type[Cov[int]]))
    static_assert(not is_disjoint_from(type[CovSub[int]], type[Cov[bool]]))

    static_assert(not is_subtype_of(type[ContraSub[bool]], type[Contra[int]]))
    static_assert(is_subtype_of(type[ContraSub[int]], type[Contra[bool]]))
    static_assert(not is_disjoint_from(type[ContraSub[int]], type[Contra[bool]]))
    static_assert(not is_disjoint_from(type[ContraSub[bool]], type[Contra[int]]))

    static_assert(not is_subtype_of(type[InvSub[bool]], type[Inv[int]]))
    static_assert(not is_subtype_of(type[InvSub[int]], type[Inv[bool]]))
    # TODO: These are disjoint.
    static_assert(not is_disjoint_from(type[InvSub[bool]], type[Inv[int]]))
    # TODO: These are disjoint.
    static_assert(not is_disjoint_from(type[InvSub[int]], type[Inv[bool]]))

def _[T]():
    static_assert(is_subtype_of(type[BivSub[T]], type[Biv[Any]]))
    static_assert(is_subtype_of(type[BivSub[Any]], type[Biv[T]]))
    static_assert(is_assignable_to(type[BivSub[T]], type[Biv[Any]]))
    static_assert(is_assignable_to(type[BivSub[Any]], type[Biv[T]]))
    static_assert(not is_disjoint_from(type[BivSub[T]], type[Biv[Any]]))

    static_assert(not is_subtype_of(type[CovSub[T]], type[Cov[Any]]))
    static_assert(not is_subtype_of(type[CovSub[Any]], type[Cov[T]]))
    static_assert(is_assignable_to(type[CovSub[T]], type[Cov[Any]]))
    static_assert(is_assignable_to(type[CovSub[Any]], type[Cov[T]]))
    static_assert(not is_disjoint_from(type[CovSub[T]], type[Cov[Any]]))

    static_assert(not is_subtype_of(type[ContraSub[T]], type[Contra[Any]]))
    static_assert(not is_subtype_of(type[ContraSub[Any]], type[Contra[T]]))
    static_assert(is_assignable_to(type[ContraSub[T]], type[Contra[Any]]))
    static_assert(is_assignable_to(type[ContraSub[Any]], type[Contra[T]]))
    static_assert(not is_disjoint_from(type[ContraSub[T]], type[Contra[Any]]))

    static_assert(not is_subtype_of(type[InvSub[T]], type[Inv[Any]]))
    static_assert(not is_subtype_of(type[InvSub[Any]], type[Inv[T]]))
    static_assert(is_assignable_to(type[InvSub[T]], type[Inv[Any]]))
    static_assert(is_assignable_to(type[InvSub[Any]], type[Inv[T]]))
    static_assert(not is_disjoint_from(type[InvSub[T]], type[Inv[Any]]))
```
