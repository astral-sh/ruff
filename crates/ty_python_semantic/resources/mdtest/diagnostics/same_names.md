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

    # error: [invalid-return-type] "Return type does not match returned value: expected `package.foo.MyClass @ src/package/foo.py:1:7`, found `package.foo.MyClass @ src/package/foo.pyi:1:7`"
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

## Callable special forms

ty distinguishes same-named classes nested in the signatures of two callable special forms.

`first.py`:

```py
from typing import Callable

class StartResponse: ...

Application = Callable[[StartResponse], int]
```

```py
from typing import Callable

try:
    from first import Application, StartResponse
except ImportError:
    class StartResponse: ...

    # error: [invalid-assignment] "Object of type `<Callable special-form '(mdtest_snippet.StartResponse, /) -> int'>` is not assignable to `<Callable special-form '(first.StartResponse, /) -> int'>`"
    Application = Callable[[StartResponse], int]
```

## Method and constructor descriptions

ty distinguishes the defining class of a bound method, unbound method, or constructor from a
same-named argument type. Method owners with no visible ambiguity remain unqualified.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
import first

class Model:
    def __init__(self, value: first.Model) -> None: ...
    def method(self, value: first.Model) -> None: ...

class Other:
    def method(self, value: first.Model) -> None: ...
```

```py
import second

def calls(value: second.Model, other: second.Other) -> None:
    # error: [invalid-argument-type] "Argument to bound method `second.Model.method` is incorrect: Expected `first.Model`, found `Literal[1]`"
    value.method(1)

    # error: [invalid-argument-type] "Argument to function `second.Model.method` is incorrect: Expected `first.Model`, found `Literal[1]`"
    second.Model.method(value, 1)

    # error: [invalid-argument-type] "Argument to `second.Model.__init__` is incorrect: Expected `first.Model`, found `Literal[1]`"
    second.Model(1)

    # No competing type named `Other` appears in this diagnostic, so its method owner stays unqualified.
    # error: [invalid-argument-type] "Argument to bound method `Other.method` is incorrect: Expected `Model`, found `Literal[1]`"
    other.method(1)
```

## Builtin class descriptions

ty distinguishes a builtin class used as a callable from a same-named argument type.

```py
import builtins

class tuple: ...

def convert(value: tuple) -> None:
    # error: [invalid-argument-type] "Argument to class `builtins.tuple` is incorrect: Expected `Iterable[Unknown]`, found `mdtest_snippet.tuple`"
    builtins.tuple(value)
```

## Identifying union members

ty uses the same qualification for a union member missing an attribute as for the complete union.

`first.py`:

```py
class Model:
    present: int
```

`second.py`:

```py
class Model: ...
```

```py
import first
import second

def missing_attribute(value: first.Model | second.Model) -> int:
    # error: [unresolved-attribute] "Attribute `present` is not defined on `second.Model` in union `first.Model | second.Model`"
    return value.present
```

## Aliased union members

ty distinguishes a union's type alias from a same-named member that does not define an attribute.

```toml
[environment]
python-version = "3.12"
```

`first.py`:

```py
class Present:
    present: int
```

`second.py`:

```py
class Model: ...
```

`alias.py`:

```py
import first
import second

type Model = first.Present | second.Model
```

```py
from alias import Model

def missing_attribute(value: Model) -> int:
    # error: [unresolved-attribute] "Attribute `present` is not defined on `second.Model` in union `alias.Model`"
    return value.present
```

## Redefined union members

When distinct union members have the same name in the same module, ty identifies the missing member
using both its source location and its module name.

`test.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    class Model:
        present: int

else:
    class Model: ...

# error: [unresolved-attribute] "Attribute `present` is not defined on `test.Model @ src/test.py:9:11` in union `test.Model @ src/test.py:5:11 | test.Model @ src/test.py:9:11`"
Model().present
```

## Attribute assignments

For ordinary and union attribute assignments, ty distinguishes the assigned class from a same-named
class appearing elsewhere in the diagnostic.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
import first
import second

class Owner:
    item: first.Model

class Other:
    item: int

def assign_attribute(owner: Owner, value: second.Model) -> None:
    # error: [invalid-assignment] "Object of type `second.Model` is not assignable to attribute `item` of type `first.Model`"
    owner.item = value

def assign_union_attribute(owner: first.Model | Other, value: second.Model) -> None:
    # error: [invalid-assignment] "Object of type `second.Model` is not assignable to attribute `item` on type `first.Model | Other`"
    owner.item = value
```

## Subscript assignments

ty distinguishes an incompatible assigned value or subscript key from a same-named class nested in
the subscripted object's type.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
import first
import second

def assign_value(values: list[first.Model], value: second.Model) -> None:
    # error: [invalid-assignment] "Invalid subscript assignment with key of type `Literal[0]` and value of type `second.Model` on object of type `list[first.Model]`"
    values[0] = value

def assign_key(values: dict[first.Model, int], key: second.Model) -> None:
    # error: [invalid-assignment] "Invalid subscript assignment with key of type `second.Model` and value of type `Literal[1]` on object of type `dict[first.Model, int]`"
    values[key] = 1
```

## Type assertions

ty distinguishes an asserted class from a same-named inferred class.

```toml
[environment]
python-version = "3.11"
```

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from typing import assert_type

import first
import second

def invalid_assertion(value: second.Model) -> None:
    assert_type(value, first.Model)  # snapshot: type-assertion-failure
```

```snapshot
error[type-assertion-failure]: Argument does not have asserted type `first.Model`
 --> src/mdtest_snippet.py:7:5
  |
7 |     assert_type(value, first.Model)  # snapshot: type-assertion-failure
  |     ^^^^^^^^^^^^-----^^^^^^^^^^^^^^
  |                 |
  |                 Inferred type is `second.Model`
info: `first.Model` and `second.Model` are not equivalent types
```

## Unspellable subtype assertions

ty distinguishes same-named classes throughout a type assertion about an unspellable intersection.

```toml
[environment]
python-version = "3.11"
```

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from typing import assert_type

import first
import second

def invalid_subtype_assertion(value: first.Model) -> None:
    if isinstance(value, second.Model):
        assert_type(value, second.Model)  # snapshot: assert-type-unspellable-subtype
```

```snapshot
error[assert-type-unspellable-subtype]: Argument does not have asserted type `second.Model`
 --> src/mdtest_snippet.py:8:9
  |
8 |         assert_type(value, second.Model)  # snapshot: assert-type-unspellable-subtype
  |         ^^^^^^^^^^^^-----^^^^^^^^^^^^^^^
  |                     |
  |                     Inferred type is `first.Model & second.Model`
info: `first.Model & second.Model` is a subtype of `second.Model`, but they are not equivalent
```

## Incompatible inherited methods

ty distinguishes a derived class from its same-named base when their inherited methods are
incompatible.

`first.py`:

```py
class Model:
    def method(self, value: int) -> int:
        return value
```

`second.py`:

```py
class Different:
    def method(self, value: str) -> str:
        return value
```

```py
import first
import second

# error: [invalid-method-override] "Base classes for class `mdtest_snippet.Model` define method `method` incompatibly: `first.Model.method` is incompatible with `Different.method`"
class Model(first.Model, second.Different): ...
```

## Conflicting metaclasses

ty distinguishes same-named classes and metaclasses throughout a metaclass-conflict diagnostic.

`first.py`:

```py
class Meta(type): ...
class Model(metaclass=Meta): ...
```

```py
import first

class OtherMeta(type): ...

# error: [conflicting-metaclass] "derived class (`mdtest_snippet.Model`) must be a subclass of the metaclasses of all its bases, but `OtherMeta` (metaclass of `mdtest_snippet.Model`) and `Meta` (metaclass of base class `first.Model`) have no subclass relationship"
class Model(first.Model, metaclass=OtherMeta): ...
class Meta(type): ...

# error: [conflicting-metaclass] "derived class (`Other`) must be a subclass of the metaclasses of all its bases, but `mdtest_snippet.Meta` (metaclass of `Other`) and `first.Meta` (metaclass of base class `Model`) have no subclass relationship"
class Other(first.Model, metaclass=Meta): ...
```

## Parameter defaults

A diagnostic that formats two types independently still disambiguates classes sharing the same name.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

`main.py`:

```py
import first
import second

# error: [invalid-parameter-default] "Default value of type `second.Model` is not assignable to annotated parameter type `first.Model`"
def accepts_model(value: first.Model = second.Model()) -> None: ...
```

## TypedDict field explanations

An explanation disambiguates both the enclosing TypedDict classes and their nested field types
across the complete diagnostic.

`first.py`:

```py
from typing import TypedDict

class Model: ...

class Record(TypedDict):
    value: Model
```

`second.py`:

```py
from typing import TypedDict

class Model: ...

class Record(TypedDict):
    value: Model
```

`main.py`:

```py
import first
import second

def update(value: first.Record, replacement: second.Record) -> None:
    value = replacement  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `second.Record` is not assignable to `first.Record`
 --> src/main.py:5:13
  |
5 |     value = replacement  # snapshot: invalid-assignment
  |     -----   ^^^^^^^^^^^ Incompatible value of type `second.Record`
  |     |
  |     Declared type `first.Record`
info: field "value" on TypedDict `second.Record` has type `second.Model` which is not assignable to type `first.Model` expected by TypedDict `first.Record`
```

## Overload signatures

Types inside different callable signatures share the diagnostic's name resolver and remain
distinguishable in explanatory notes.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

`main.py`:

```py
from typing import overload
import first
import second

@overload
def convert(value: first.Model) -> None: ...  # snapshot: invalid-overload
@overload
def convert(value: int) -> None: ...
def convert(value: second.Model | int) -> None: ...
```

```snapshot
error[invalid-overload]: Implementation does not accept all arguments of this overload
 --> src/main.py:6:5
  |
6 | def convert(value: first.Model) -> None: ...  # snapshot: invalid-overload
  |     ^^^^^^^
7 | @overload
8 | def convert(value: int) -> None: ...
9 | def convert(value: second.Model | int) -> None: ...
  |     ------- Implementation defined here
info: Implementation signature `(value: second.Model | int) -> None` is not assignable to overload signature `(value: first.Model) -> None`
info: parameter `value` has an incompatible type: `first.Model` is not assignable to `second.Model | int`
```

## Specialized runtime classes

Specialized runtime values retain the identity of their actual class even when their formatter
normally uses a short, hand-written name.

```toml
[environment]
python-version = "3.13"
```

`custom.py`:

```py
class property: ...
class range: ...
class TypeAliasType: ...
class TypeVar: ...
class ParamSpec: ...
class TypeVarTuple: ...
class UnionType: ...
class module: ...
```

`main.py`:

```py
from typing import ParamSpec, TypeVar, TypeVarTuple
import os
import custom

class Owner:
    @property
    def value(self) -> int:
        return 1

type Alias = int
T = TypeVar("T")
P = ParamSpec("P")
Ts = TypeVarTuple("Ts")

# error: [invalid-assignment] "Object of type `builtins.property` is not assignable to `custom.property`"
wrong_property: custom.property = Owner.value

# error: [invalid-assignment] "Object of type `builtins.range` is not assignable to `custom.range`"
wrong_range: custom.range = range(3)

# error: [invalid-assignment] "Object of type `typing.TypeAliasType` is not assignable to `custom.TypeAliasType`"
wrong_alias: custom.TypeAliasType = Alias

# error: [invalid-assignment] "Object of type `typing.TypeVar` is not assignable to `custom.TypeVar`"
wrong_typevar: custom.TypeVar = T

# error: [invalid-assignment] "Object of type `typing.ParamSpec` is not assignable to `custom.ParamSpec`"
wrong_paramspec: custom.ParamSpec = P

# error: [invalid-assignment] "Object of type `typing.TypeVarTuple` is not assignable to `custom.TypeVarTuple`"
wrong_typevartuple: custom.TypeVarTuple = Ts

# error: [invalid-assignment] "Object of type `<types.UnionType special-form 'int | str'>` is not assignable to `custom.UnionType`"
wrong_union: custom.UnionType = int | str

# error: [invalid-assignment] "Object of type `<module 'os'>` is not assignable to `custom.module`"
wrong_module: custom.module = os
```

## Specialized alias values

A specialized PEP 695 alias retains the identity of the alias itself, not just the identities of its
type arguments.

```toml
[environment]
python-version = "3.13"
```

`first.py`:

```py
class Alias: ...
```

`main.py`:

```py
import first

type Alias[T] = list[T]

# error: [invalid-assignment] "Object of type `<type alias 'main.Alias[int]'>` is not assignable to `first.Alias`"
wrong_alias: first.Alias = Alias[int]
```

## Partial instances

The specialized formatter for `functools.partial` preserves the identity of its runtime class.

`custom.py`:

```py
class partial: ...
```

`main.py`:

```py
from functools import partial
import custom

def identity(value: int) -> int:
    return value

# error: [invalid-assignment] "Object of type `functools.partial[(value: int) -> int]` is not assignable to `custom.partial`"
wrong: custom.partial = partial(identity)
```

## Generator return classes

Generator diagnostics retain the existing fully qualified runtime class spelling without introducing
spurious unknown type arguments.

`custom.py`:

```py
class GeneratorType: ...
class AsyncGeneratorType: ...
```

`main.py`:

```py
import custom

# error: [invalid-return-type] "Return type does not match returned value: expected `custom.GeneratorType`, found `types.GeneratorType`"
def synchronous() -> custom.GeneratorType:
    yield 1

# error: [invalid-return-type] "Return type does not match returned value: expected `custom.AsyncGeneratorType`, found `types.AsyncGeneratorType`"
async def asynchronous() -> custom.AsyncGeneratorType:
    yield 1
```

## Generator yield and send types

Yielded values and delegated send types retain their class identities in concise messages and
annotations until the complete diagnostic is finalized.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

`main.py`:

```py
from collections.abc import Generator
import first
import second

def yielded() -> Generator[first.Model, None, None]:
    # error: [invalid-yield] "Yield type `second.Model` does not match annotated yield type `first.Model`"
    yield second.Model()

def accepts_second() -> Generator[int, second.Model, None]:
    yield 1

def delegates() -> Generator[int, first.Model, None]:
    # error: [invalid-yield] "Send type `second.Model` does not match annotated send type `first.Model`"
    yield from accepts_second()
```

## Runtime wrapper owners

A method wrapper's owning runtime class participates in name resolution alongside a separately
displayed expected class.

`custom.py`:

```py
class property: ...
```

`main.py`:

```py
import custom

class Owner:
    @property
    def value(self) -> int:
        return 1

# error: [invalid-assignment] "Object of type `<method-wrapper '__get__' of builtins.property 'value'>` is not assignable to `custom.property`"
wrong_wrapper: custom.property = Owner.value.__get__

# error: [invalid-assignment] "Object of type `<wrapper-descriptor '__get__' of 'builtins.property' objects>` is not assignable to `custom.property`"
wrong_descriptor: custom.property = property.__get__
```

## Conflicting declaration enumerations

A human-readable enumeration formats each declared type directly into the diagnostic instead of
flattening the list into a string before name resolution.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

`main.py`:

```py
import first
import second

def conflicting(flag: bool) -> None:
    if flag:
        value: first.Model
    else:
        value: second.Model

    # error: [conflicting-declarations] "Conflicting declared types for `value`: `first.Model` and `second.Model`"
    value = first.Model()
```

## Class inheritance

A derived class and its same-named base remain distinguishable in class-validation diagnostics,
including protocol and TypedDict inheritance.

`first.py`:

```py
from typing import final

@final
class Model: ...

class Interface: ...
class Record: ...
```

`main.py`:

```py
from typing import Protocol, TypedDict
import first

# error: [subclass-of-final-class] "Class `main.Model` cannot inherit from final class `first.Model`"
class Model(first.Model): ...

# error: [invalid-protocol] "Protocol class `main.Interface` cannot inherit from non-protocol class `first.Interface`"
class Interface(first.Interface, Protocol): ...

# error: [invalid-typed-dict-header] "TypedDict class `main.Record` can only inherit from TypedDict classes: `first.Record` is not a `TypedDict` class"
class Record(first.Record, TypedDict): ...
```

## Dynamic class inheritance

Dynamic classes retain their own class identity while their base classes are validated.

`first.py`:

```py
from typing import final

@final
class Model: ...
```

`main.py`:

```py
import first

# error: [subclass-of-final-class] "Class `main.Model` cannot inherit from final class `first.Model`"
Model = type("Model", (first.Model,), {})
```

## Dynamic metaclass conflicts

Conflicting metaclasses and their originating classes remain distinguishable while their existing
dynamic class representations are preserved.

`first.py`:

```py
class Meta(type): ...
class Model(metaclass=Meta): ...
```

`second.py`:

```py
class Meta(type): ...
class Model(metaclass=Meta): ...
```

`main.py`:

```py
import first
import second

# error: [conflicting-metaclass] "`first.Meta` (metaclass of base class `<class 'first.Model'>`) and `second.Meta` (metaclass of base class `<class 'second.Model'>`)"
Dynamic = type("Dynamic", (first.Model, second.Model), {})
```

## TypedDict openness

Diagnostics that compare a closed TypedDict subclass with a same-named base preserve both class
identities.

```toml
[environment]
python-version = "3.15"
```

`first.py`:

```py
from typing import TypedDict

class Record(TypedDict, closed=True): ...
```

`main.py`:

```py
import first

# error: [invalid-typed-dict-header] "TypedDict `main.Record` must remain closed because base `first.Record` is closed"
class Record(first.Record, closed=False): ...
```
