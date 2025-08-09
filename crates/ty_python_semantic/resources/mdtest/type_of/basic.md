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
class Foo(type[int]): ...

# TODO: should be `tuple[<class 'Foo'>, <class 'type'>, <class 'object'>]
reveal_type(Foo.__mro__)  # revealed: tuple[<class 'Foo'>, @Todo(GenericAlias instance), <class 'object'>]
```

## Display of generic `type[]` types

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, TypeVar

class Foo[T]: ...

S = TypeVar("S")

class Bar(Generic[S]): ...

def _(x: Foo[int], y: Bar[str], z: list[bytes]):
    reveal_type(type(x))  # revealed: type[Foo[int]]
    reveal_type(type(y))  # revealed: type[Bar[str]]
    reveal_type(type(z))  # revealed: type[list[bytes]]
```

## `@final` classes

`type[]` types are eagerly converted to class-literal types if a class decorated with `@final` is
used as the type argument. This applies to standard-library classes and user-defined classes. The
same also applies to enum classes with members, which are implicitly final:

```toml
[environment]
python-version = "3.10"
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
