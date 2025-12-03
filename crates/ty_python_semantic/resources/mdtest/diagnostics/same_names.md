# Identical type display names in diagnostics

ty prints the fully qualified name to disambiguate objects with the same name.

## Nested class

`test.py`:

```py
class A:
    class B:
        pass

class C:
    class B:
        pass

a: A.B = C.B()  # error: [invalid-assignment] "Object of type `test.C.B` is not assignable to `test.A.B`"
```

## Nested class in function

`test.py`:

```py
class B:
    pass

def f(b: B):
    class B:
        pass

    # error: [invalid-assignment] "Object of type `test.<locals of function 'f'>.B` is not assignable to `test.B`"
    b = B()
```

## Class from different modules

```py
import a
import b

df: a.DataFrame = b.DataFrame()  # error: [invalid-assignment] "Object of type `b.DataFrame` is not assignable to `a.DataFrame`"

def _(dfs: list[b.DataFrame]):
    # error: [invalid-assignment] "Object of type `list[b.DataFrame]` is not assignable to `list[a.DataFrame]`"
    dataframes: list[a.DataFrame] = dfs
```

`a.py`:

```py
class DataFrame:
    pass
```

`b.py`:

```py
class DataFrame:
    pass
```

## Class from different module with the same qualified name

`package/__init__.py`:

```py
from .foo import MyClass

def make_MyClass() -> MyClass:
    return MyClass()
```

`package/foo.pyi`:

```pyi
class MyClass: ...
```

`package/foo.py`:

```py
class MyClass: ...

def get_MyClass() -> MyClass:
    from . import make_MyClass

    # error: [invalid-return-type] "Return type does not match returned value: expected `package.foo.MyClass @ src/package/foo.py:1`, found `package.foo.MyClass @ src/package/foo.pyi:1`"
    return make_MyClass()
```

## Enum from different modules

```py
import status_a
import status_b

# error: [invalid-assignment] "Object of type `Literal[status_b.Status.ACTIVE]` is not assignable to `status_a.Status`"
s: status_a.Status = status_b.Status.ACTIVE
```

`status_a.py`:

```py
from enum import Enum

class Status(Enum):
    ACTIVE = 1
    INACTIVE = 2
```

`status_b.py`:

```py
from enum import Enum

class Status(Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
```

## Nested enum

`test.py`:

```py
from enum import Enum

class A:
    class B(Enum):
        ACTIVE = "active"
        INACTIVE = "inactive"

class C:
    class B(Enum):
        ACTIVE = "active"
        INACTIVE = "inactive"

# error: [invalid-assignment] "Object of type `Literal[test.C.B.ACTIVE]` is not assignable to `test.A.B`"
a: A.B = C.B.ACTIVE
```

## Class literals

```py
import cls_a
import cls_b

# error: [invalid-assignment] "Object of type `<class 'cls_b.Config'>` is not assignable to `type[cls_a.Config]`"
config_class: type[cls_a.Config] = cls_b.Config
```

`cls_a.py`:

```py
class Config:
    pass
```

`cls_b.py`:

```py
class Config:
    pass
```

## Generic aliases

```py
import generic_a
import generic_b

# error: [invalid-assignment] "Object of type `<class 'generic_b.Container[int]'>` is not assignable to `type[generic_a.Container[int]]`"
container: type[generic_a.Container[int]] = generic_b.Container[int]
```

`generic_a.py`:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Container(Generic[T]):
    pass
```

`generic_b.py`:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Container(Generic[T]):
    pass
```

## Protocols

### Differing members

`bad.py`:

```py
from typing import Protocol, TypeVar

T_co = TypeVar("T_co", covariant=True)

class Iterator(Protocol[T_co]):
    def __nexxt__(self) -> T_co: ...

def bad() -> Iterator[str]:
    raise NotImplementedError
```

`main.py`:

```py
from typing import Iterator

def f() -> Iterator[str]:
    import bad

    # error: [invalid-return-type] "Return type does not match returned value: expected `typing.Iterator[str]`, found `bad.Iterator[str]"
    return bad.bad()
```

### Same members but with different types

```py
from typing import Protocol
import proto_a
import proto_b

def _(drawable_b: proto_b.Drawable):
    # error: [invalid-assignment] "Object of type `proto_b.Drawable` is not assignable to `proto_a.Drawable`"
    drawable: proto_a.Drawable = drawable_b
```

`proto_a.py`:

```py
from typing import Protocol

class Drawable(Protocol):
    def draw(self) -> None: ...
```

`proto_b.py`:

```py
from typing import Protocol

class Drawable(Protocol):
    def draw(self) -> int: ...
```

## TypedDict

```py
from typing import TypedDict
import dict_a
import dict_b

def _(b_person: dict_b.Person):
    # error: [invalid-assignment] "Object of type `dict_b.Person` is not assignable to `dict_a.Person`"
    person_var: dict_a.Person = b_person
```

`dict_a.py`:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
```

`dict_b.py`:

```py
from typing import TypedDict

class Person(TypedDict):
    name: bytes
```

## Tuple specializations

`module.py`:

```py
class Model: ...
```

```py
class Model: ...

def get_models_tuple() -> tuple[Model]:
    from module import Model

    # error: [invalid-return-type] "Return type does not match returned value: expected `tuple[mdtest_snippet.Model]`, found `tuple[module.Model]`"
    return (Model(),)
```
