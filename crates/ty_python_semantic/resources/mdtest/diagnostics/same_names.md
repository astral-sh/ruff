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

## Assignments and declarations

Attribute assignments, annotated instance attributes, declarations, conflicting declarations, and
assignment annotations distinguish unrelated classes with the same name.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

`test.py`:

```py
import first
import second

class Owner:
    item: first.Model

    def __init__(self, value: second.Model) -> None:
        # error: [invalid-assignment] "Object of type `second.Model` is not assignable to `first.Model`"
        self.item: first.Model = value

def assign_attribute(owner: Owner, value: second.Model) -> None:
    # error: [invalid-assignment] "Object of type `second.Model` is not assignable to attribute `item` of type `first.Model`"
    owner.item = value

declared_later = second.Model()
# error: [invalid-declaration] "Cannot declare type `first.Model` for inferred type `second.Model`"
declared_later: first.Model

def conflicting_declarations(flag: bool) -> None:
    if flag:
        conflicting: first.Model
    else:
        conflicting: second.Model
    # error: [conflicting-declarations] "Conflicting declared types for `conflicting`: `first.Model` and `second.Model`"
    conflicting = first.Model()

def assignment_annotations(value: first.Model, replacement: second.Model) -> None:
    value = replacement  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `second.Model` is not assignable to `first.Model`
  --> src/test.py:28:13
   |
28 |     value = replacement  # snapshot: invalid-assignment
   |     -----   ^^^^^^^^^^^ Incompatible value of type `second.Model`
   |     |
   |     Declared type `first.Model`
```

## Descriptor access and assignments

Descriptor diagnostics distinguish the owner, descriptor, and invalid argument even when all three
classes have the same name.

`descriptor.py`:

```py
class Model:
    def __set__(self, instance: object, value: int) -> None: ...
    def __get__(self, instance: int, owner: object) -> int:
        return 1
```

`owner.py`:

```py
import descriptor

class Model:
    item: descriptor.Model = descriptor.Model()
```

`value.py`:

```py
class Model: ...
```

```py
import owner
import value

instance = owner.Model()
instance.item = value.Model()  # snapshot: invalid-assignment
instance.item  # snapshot: invalid-attribute-access
```

```snapshot
error[invalid-assignment]: Invalid assignment to data descriptor attribute `item` on type `owner.Model`
 --> src/mdtest_snippet.py:5:17
  |
5 | instance.item = value.Model()  # snapshot: invalid-assignment
  |                 ^^^^^^^^^^^^^ Expected `int`, found `value.Model`
info: Argument to function `descriptor.Model.__set__` is incorrect
info: This assignment implicitly calls `__set__` on a descriptor of type `descriptor.Model`
info: Function defined here
 --> src/descriptor.py:2:9
  |
2 |     def __set__(self, instance: object, value: int) -> None: ...
  |         ^^^^^^^                         ---------- Parameter declared here


error[invalid-attribute-access]: Invalid access to descriptor attribute `item` on type `owner.Model`
 --> src/mdtest_snippet.py:6:1
  |
6 | instance.item  # snapshot: invalid-attribute-access
  | ^^^^^^^^ Expected `int`, found `owner.Model`
info: Argument to function `descriptor.Model.__get__` is incorrect
info: This access implicitly calls `__get__` on a descriptor of type `descriptor.Model`
info: Function defined here
 --> src/descriptor.py:3:9
  |
3 |     def __get__(self, instance: int, owner: object) -> int:
  |         ^^^^^^^       ------------- Parameter declared here
```

## Function defaults, assertions, and narrowing

The expected and actual types remain distinct in parameter defaults, `assert_type`, and `TypeIs`
definitions.

```toml
[environment]
python-version = "3.13"
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
from typing import TypeIs, assert_type

import first
import second

# error: [invalid-parameter-default] "Default value of type `second.Model` is not assignable to annotated parameter type `first.Model`"
def invalid_default(value: first.Model = second.Model()) -> None: ...
def invalid_assertion(value: second.Model) -> None:
    # error: [type-assertion-failure] "Type `second.Model` does not match asserted type `first.Model`"
    assert_type(value, first.Model)

# error: [invalid-type-guard-definition] "Narrowed type `second.Model` is not assignable to the declared parameter type `first.Model`"
def invalid_type_is(value: first.Model) -> TypeIs[second.Model]:
    return True
```

## Type variable bounds, constraints, and defaults

Diagnostics for explicit specializations, inferred call specializations, and type-variable defaults
distinguish the types being compared.

```toml
[environment]
python-version = "3.13"
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
from typing import Generic, TypeVar

import first
import second

BoundedT = TypeVar("BoundedT", bound=first.Model)
ConstrainedT = TypeVar("ConstrainedT", first.Model, int)

class Bounded(Generic[BoundedT]): ...
class Constrained(Generic[ConstrainedT]): ...

# error: [invalid-type-arguments] "Type `second.Model` is not assignable to upper bound `first.Model`"
invalid_bound: Bounded[second.Model]

# error: [invalid-type-arguments] "Type `second.Model` does not satisfy constraints `first.Model`, `int`"
invalid_constraint: Constrained[second.Model]

def requires_bound(value: BoundedT) -> BoundedT:
    return value

def requires_constraint(value: ConstrainedT) -> ConstrainedT:
    return value

# error: [invalid-argument-type] "Argument type `second.Model` does not satisfy upper bound `first.Model`"
requires_bound(second.Model())

# error: [invalid-argument-type] "Argument type `second.Model` does not satisfy constraints (`first.Model`, `int`)"
requires_constraint(second.Model())

# error: [invalid-type-variable-default] "upper bound `second.Model` is not assignable to `first.Model`"
def invalid_default[First: second.Model, Second: first.Model = First]() -> None: ...
```

## Inconsistent generic bases

Diagnostics for conflicting specializations of the same generic ancestor distinguish their type
arguments.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from typing import Generic, TypeVar

import first
import second

T = TypeVar("T")

class SharedBase(Generic[T]): ...
class FirstBase(SharedBase[first.Model]): ...
class SecondBase(SharedBase[second.Model]): ...

# error: [invalid-generic-class] "class cannot inherit from both `SharedBase[second.Model]` and `SharedBase[first.Model]`"
class InconsistentBases(FirstBase, SecondBase): ...
```

## TypedDict inheritance and assignments

Inherited field overrides, explicit extra items, and assignments all compare the declared and actual
item types together.

```toml
[environment]
python-version = "3.15"
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
from typing import ReadOnly, TypedDict

import first
import second

class Mapping(TypedDict):
    item: first.Model

class OverrideMapping(Mapping):
    # error: [invalid-typed-dict-field] "Inherited mutable field type `first.Model` is incompatible with `second.Model`"
    item: second.Model

class ReadOnlyMapping(TypedDict):
    item: ReadOnly[first.Model]

class ReadOnlyOverrideMapping(ReadOnlyMapping):
    # error: [invalid-typed-dict-field] "Inherited read-only field type `first.Model` is not assignable from `second.Model`"
    item: second.Model

def assign_item(mapping: Mapping, value: second.Model) -> None:
    # error: [invalid-assignment] "declared type `first.Model` on TypedDict `Mapping`: value of type `second.Model`"
    mapping["item"] = value

class BaseExtras(TypedDict, extra_items=ReadOnly[first.Model]): ...

# error: [invalid-typed-dict-header] "Extra items type `second.Model` is not assignable to `first.Model`"
class ChildExtras(BaseExtras, extra_items=second.Model): ...

# error: [invalid-typed-dict-header] "Item `item` of type `second.Model` is not assignable to extra items type `first.Model`"
class ChildField(BaseExtras):
    item: second.Model
```

## TypedDict inheritance openness

TypedDict openness diagnostics distinguish a subclass from a same-named base class.

```toml
[environment]
python-version = "3.15"
```

`first.py`:

```py
from typing import ReadOnly, TypedDict

class Closed(TypedDict, closed=True): ...
class ReadOnlyExtras(TypedDict, extra_items=ReadOnly[int]): ...
class MutableExtras(TypedDict, extra_items=int): ...
class MutableExtrasType(TypedDict, extra_items=int): ...
```

```py
import first

# error: [invalid-typed-dict-header] "TypedDict `mdtest_snippet.Closed` must remain closed because base `first.Closed` is closed"
class Closed(first.Closed, closed=False): ...

# error: [invalid-typed-dict-header] "TypedDict `mdtest_snippet.ReadOnlyExtras` cannot be open because base `first.ReadOnlyExtras` has extra items"
class ReadOnlyExtras(first.ReadOnlyExtras, closed=False): ...

# error: [invalid-typed-dict-header] "TypedDict `mdtest_snippet.MutableExtras` must preserve mutable extra items from base `first.MutableExtras`"
class MutableExtras(first.MutableExtras, closed=True): ...

# error: [invalid-typed-dict-header] "TypedDict `mdtest_snippet.MutableExtrasType` must preserve mutable extra items type `int` from base `first.MutableExtrasType`"
class MutableExtrasType(first.MutableExtrasType, extra_items=str): ...
```

## TypedDict assignment annotations

Receiver annotations distinguish the TypedDict from an incompatible value with the same class name.

`first.py`:

```py
from typing import TypedDict

class Model(TypedDict):
    value: int
```

`second.py`:

```py
class Model: ...
```

```py
import first
import second

def assign(value: first.Model, replacement: second.Model) -> None:
    value["value"] = replacement  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Invalid assignment to key "value" with declared type `int` on TypedDict `first.Model`
 --> src/mdtest_snippet.py:5:22
  |
5 |     value["value"] = replacement  # snapshot: invalid-assignment
  |     ----- -------    ^^^^^^^^^^^ value of type `second.Model`
  |     |     |
  |     |     key has declared type `int`
  |     TypedDict `first.Model`
info: Item declaration
 --> src/first.py:4:5
  |
4 |     value: int
  |     ---------- Item declared here
```

## TypedDict constructor item types

Constructor validation distinguishes extra-item types, values supplied through non-literal keys, and
TypedDict names that collide with invalid key types.

```toml
[environment]
python-version = "3.15"
```

`first.py`:

```py
from typing import TypedDict

class Model: ...
class Record(TypedDict): ...
```

`second.py`:

```py
class Model: ...
class Record: ...
```

```py
from typing import TypedDict

import first
import second

class Source(TypedDict, extra_items=second.Model): ...
class Target(TypedDict, extra_items=first.Model): ...

class TargetWithField(TypedDict, extra_items=first.Model):
    item: first.Model

def constructors(source: Source, key: str, value: second.Model) -> None:
    # error: [invalid-argument-type] "extra items of type `second.Model` that are not assignable to extra items type `first.Model`"
    Target(**source)

    # error: [missing-typed-dict-key]
    # error: [invalid-argument-type] "extra items of type `second.Model` that are not assignable to item `item` with type `first.Model`"
    TargetWithField(**source)

    # error: [invalid-argument-type] "Value of type `second.Model` is not assignable to arbitrary key value type `first.Model`"
    Target({key: value})

    # error: [invalid-key] "TypedDict `first.Record` requires string keys, got key of type `second.Record`"
    first.Record({second.Record(): value})
```

## Assignability explanations

Explanatory notes distinguish both same-named containers and the types nested inside them.

`first.py`:

```py
from typing import Protocol, TypedDict

class Model: ...

class Record(TypedDict):
    value: Model

class Interface(Protocol):
    value: Model
```

`second.py`:

```py
from typing import Protocol, TypedDict

class Model: ...

class Record(TypedDict):
    value: Model

class Interface(Protocol):
    value: Model
```

```py
from collections.abc import Callable

import first
import second

def typed_dict(value: first.Record) -> second.Record:
    return value  # snapshot: invalid-return-type

def callable_return(value: Callable[[], first.Model]) -> Callable[[], second.Model]:
    return value  # snapshot: invalid-return-type

def callable_parameter(value: Callable[[first.Model], None]) -> Callable[[second.Model], None]:
    return value  # snapshot: invalid-return-type

def tuple_element(value: tuple[first.Model]) -> tuple[second.Model]:
    return value  # snapshot: invalid-return-type

def protocol(value: first.Interface) -> second.Interface:
    return value  # snapshot: invalid-return-type
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:7:12
  |
6 | def typed_dict(value: first.Record) -> second.Record:
  |                                        ------------- Expected `second.Record` because of return type
7 |     return value  # snapshot: invalid-return-type
  |            ^^^^^ expected `second.Record`, found `first.Record`
info: field "value" on TypedDict `first.Record` has type `first.Model` which is not assignable to type `second.Model` expected by TypedDict `second.Record`


error[invalid-return-type]: Return type does not match returned value
  --> src/mdtest_snippet.py:10:12
   |
 9 | def callable_return(value: Callable[[], first.Model]) -> Callable[[], second.Model]:
   |                                                          -------------------------- Expected `() -> second.Model` because of return type
10 |     return value  # snapshot: invalid-return-type
   |            ^^^^^ expected `() -> second.Model`, found `() -> first.Model`
info: incompatible return types: `first.Model` is not assignable to `second.Model`


error[invalid-return-type]: Return type does not match returned value
  --> src/mdtest_snippet.py:13:12
   |
12 | def callable_parameter(value: Callable[[first.Model], None]) -> Callable[[second.Model], None]:
   |                                                                 ------------------------------ Expected `(second.Model, /) -> None` because of return type
13 |     return value  # snapshot: invalid-return-type
   |            ^^^^^ expected `(second.Model, /) -> None`, found `(first.Model, /) -> None`
info: the first parameter has an incompatible type: `second.Model` is not assignable to `first.Model`


error[invalid-return-type]: Return type does not match returned value
  --> src/mdtest_snippet.py:16:12
   |
15 | def tuple_element(value: tuple[first.Model]) -> tuple[second.Model]:
   |                                                 ------------------- Expected `tuple[second.Model]` because of return type
16 |     return value  # snapshot: invalid-return-type
   |            ^^^^^ expected `tuple[second.Model]`, found `tuple[first.Model]`
info: the first tuple element is not compatible: `first.Model` is not assignable to `second.Model`


error[invalid-return-type]: Return type does not match returned value
  --> src/mdtest_snippet.py:19:12
   |
18 | def protocol(value: first.Interface) -> second.Interface:
   |                                         ---------------- Expected `second.Interface` because of return type
19 |     return value  # snapshot: invalid-return-type
   |            ^^^^^ expected `second.Interface`, found `first.Interface`
info: protocol `first.Interface` is not assignable to protocol `second.Interface`
info: └── protocol member `value` is incompatible
info:     └── read type `first.Model` is not assignable to `second.Model`
```

## Assignability explanations preserve enclosing distinctions

An explanatory note identifies a type that also appears in the enclosing signatures even when the
other type in that particular note has a different name.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from collections.abc import Callable

import first
import second

def incompatible_return(
    value: Callable[[first.Model], first.Model],
) -> Callable[[second.Model], int]:
    return value  # snapshot: invalid-return-type
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:9:12
  |
8 | ) -> Callable[[second.Model], int]:
  |      ----------------------------- Expected `(second.Model, /) -> int` because of return type
9 |     return value  # snapshot: invalid-return-type
  |            ^^^^^ expected `(second.Model, /) -> int`, found `(first.Model, /) -> first.Model`
info: incompatible return types: `first.Model` is not assignable to `int`
```

## Nested protocol member types

Explanatory notes distinguish same-named members of a union nested inside an incompatible protocol
attribute.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from typing import Protocol

import first
import second

class ReadOnly:
    @property
    def item(self) -> first.Model | second.Model:
        raise NotImplementedError

class Writable(Protocol):
    item: first.Model | second.Model

def incompatible_write(value: ReadOnly) -> Writable:
    return value  # snapshot: invalid-return-type
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
  --> src/mdtest_snippet.py:15:12
   |
14 | def incompatible_write(value: ReadOnly) -> Writable:
   |                                            -------- Expected `Writable` because of return type
15 |     return value  # snapshot: invalid-return-type
   |            ^^^^^ expected `Writable`, found `ReadOnly`
info: type `ReadOnly` is not assignable to protocol `Writable`
info: └── protocol member `item` is incompatible
info:     └── the member does not accept writes of type `first.Model | second.Model`
```

## Assignability explanations for missing members

Explanatory notes identify the same-named protocol or TypedDict that lacks a required member.

`first.py`:

```py
from typing import Protocol, TypedDict

class Interface(Protocol):
    member: int

class Record(TypedDict):
    required: int
```

`second.py`:

```py
from typing import TypedDict

class Interface: ...

class Record(TypedDict):
    other: int
```

```py
import first
import second

def missing_protocol_member(value: second.Interface) -> first.Interface:
    return value  # snapshot: invalid-return-type

def missing_typed_dict_field(value: second.Record) -> first.Record:
    return value  # snapshot: invalid-return-type
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:5:12
  |
4 | def missing_protocol_member(value: second.Interface) -> first.Interface:
  |                                                         --------------- Expected `first.Interface` because of return type
5 |     return value  # snapshot: invalid-return-type
  |            ^^^^^ expected `first.Interface`, found `second.Interface`
info: type `second.Interface` is not assignable to protocol `first.Interface`
info: └── protocol member `member` is not defined on type `second.Interface`


error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:8:12
  |
7 | def missing_typed_dict_field(value: second.Record) -> first.Record:
  |                                                       ------------ Expected `first.Record` because of return type
8 |     return value  # snapshot: invalid-return-type
  |            ^^^^^ expected `first.Record`, found `second.Record`
info: required field "required" is not present in source TypedDict `second.Record`
```

## Assignability help messages

Help messages distinguish same-named TypedDicts, mapping item types, and declaring protocols.

`first.py`:

```py
from typing import Protocol, TypedDict

class Empty(TypedDict): ...

class Interface(Protocol):
    def run(self, expected: int) -> None: ...
```

`second.py`:

```py
class Empty: ...

class Interface:
    def run(self, actual: int) -> None: ...
```

```py
from collections.abc import Mapping

import first
import second

def open_typed_dict(value: first.Empty) -> Mapping[str, second.Empty]:
    return value  # snapshot: invalid-return-type

def protocol_parameter(value: second.Interface) -> first.Interface:
    return value  # snapshot: invalid-return-type
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:7:12
  |
6 | def open_typed_dict(value: first.Empty) -> Mapping[str, second.Empty]:
  |                                            -------------------------- Expected `Mapping[str, second.Empty]` because of return type
7 |     return value  # snapshot: invalid-return-type
  |            ^^^^^ expected `Mapping[str, second.Empty]`, found `first.Empty`
info: TypedDict `first.Empty` is not assignable to `Mapping[str, second.Empty]`
help: `first.Empty` would be assignable to `Mapping[str, second.Empty]` if it were declared with `closed=True`, but TypedDicts are open by default
help: A subclass of `first.Empty` could validly add a new field of an arbitrary type, violating subtyping with `Mapping[str, second.Empty]`


error[invalid-return-type]: Return type does not match returned value
  --> src/mdtest_snippet.py:10:12
   |
 9 | def protocol_parameter(value: second.Interface) -> first.Interface:
   |                                                    --------------- Expected `first.Interface` because of return type
10 |     return value  # snapshot: invalid-return-type
   |            ^^^^^ expected `first.Interface`, found `second.Interface`
info: type `second.Interface` is not assignable to protocol `first.Interface`
info: └── protocol member `run` is incompatible
info:     └── the parameter named `actual` does not match `expected` (and can be used as a keyword parameter)
help: `second.Interface` might be assignable to `first.Interface` if the parameter `expected` were made positional-only in `first.Interface.run`
```

## Nested assignability help messages

Help messages distinguish types introduced by incompatible protocol members, even when those types
do not appear in the enclosing diagnostic.

`first.py`:

```py
from typing import Protocol, TypedDict

class Empty(TypedDict): ...

class MappingInterface(Protocol):
    def method(self) -> Empty: ...

class Interface(Protocol):
    def method(self, expected: int) -> None: ...
```

`second.py`:

```py
from collections.abc import Mapping
from typing import Protocol

class Empty: ...

class MappingInterface(Protocol):
    def method(self) -> Mapping[str, Empty]: ...

class Interface:
    def method(self, actual: int) -> None: ...
```

```py
from typing import Protocol

import first
import second

def incompatible_mapping(value: first.MappingInterface) -> second.MappingInterface:
    return value  # snapshot: invalid-return-type

class Source(Protocol):
    item: second.Interface

class Target(Protocol):
    item: first.Interface

def incompatible_protocol(value: Source) -> Target:
    return value  # snapshot: invalid-return-type
```

```snapshot
error[invalid-return-type]: Return type does not match returned value
 --> src/mdtest_snippet.py:7:12
  |
6 | def incompatible_mapping(value: first.MappingInterface) -> second.MappingInterface:
  |                                                            ----------------------- Expected `second.MappingInterface` because of return type
7 |     return value  # snapshot: invalid-return-type
  |            ^^^^^ expected `second.MappingInterface`, found `first.MappingInterface`
info: protocol `first.MappingInterface` is not assignable to protocol `second.MappingInterface`
info: └── protocol member `method` is incompatible
info:     └── incompatible return types: `first.Empty` is not assignable to `Mapping[str, second.Empty]`
info:         └── TypedDict `first.Empty` is not assignable to `Mapping[str, second.Empty]`
help: `first.Empty` would be assignable to `Mapping[str, second.Empty]` if it were declared with `closed=True`, but TypedDicts are open by default
help: A subclass of `first.Empty` could validly add a new field of an arbitrary type, violating subtyping with `Mapping[str, second.Empty]`


error[invalid-return-type]: Return type does not match returned value
  --> src/mdtest_snippet.py:16:12
   |
15 | def incompatible_protocol(value: Source) -> Target:
   |                                             ------ Expected `Target` because of return type
16 |     return value  # snapshot: invalid-return-type
   |            ^^^^^ expected `Target`, found `Source`
info: protocol `Source` is not assignable to protocol `Target`
info: └── protocol member `item` is incompatible
info:     └── read type `second.Interface` is not assignable to `first.Interface`
info:         └── type `second.Interface` is not assignable to protocol `first.Interface`
info:             └── protocol member `method` is incompatible
info:                 └── the parameter named `actual` does not match `expected` (and can be used as a keyword parameter)
help: `second.Interface` might be assignable to `first.Interface` if the parameter `expected` were made positional-only in `first.Interface.method`
```

## Overload signature explanations

Diagnostics distinguish same-named types in the implementation and overload signatures, including
their parameters, return types, and explanatory notes.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from typing import overload

import first
import second

@overload
def overloaded(value: first.Model) -> first.Model: ...  # snapshot: invalid-overload
@overload
def overloaded(value: int) -> int: ...  # error: [invalid-overload]
def overloaded(value: second.Model) -> second.Model:
    return value
```

```snapshot
error[invalid-overload]: Overload signature is not consistent with implementation
  --> src/mdtest_snippet.py:7:5
   |
 7 | def overloaded(value: first.Model) -> first.Model: ...  # snapshot: invalid-overload
   |     ^^^^^^^^^^
 8 | @overload
 9 | def overloaded(value: int) -> int: ...  # error: [invalid-overload]
10 | def overloaded(value: second.Model) -> second.Model:
   |     ---------- Implementation defined here
info: Implementation signature `(value: second.Model) -> second.Model` is not assignable to overload signature `(value: first.Model) -> first.Model`
info: parameter `value` has an incompatible type: `first.Model` is not assignable to `second.Model`
info: Overload returns `first.Model`, which is not assignable to implementation return type `second.Model`
```

## Invalid super arguments

Diagnostics for invalid `super()` arguments distinguish both arguments and same-named types nested
within a generic alias.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from typing import TypeVar

import first
import second

def invalid_super(value: second.Model) -> None:
    # error: [invalid-super-argument] "`second.Model` is not an instance or subclass of `<class 'first.Model'>`"
    super(first.Model, value)

def invalid_generic_alias() -> None:
    # Revealing the type of an ordinary list instance already distinguishes these classes:
    #
    #     values: list[first.Model | second.Model] = []
    #     reveal_type(values)
    #
    # The expression passed to super() below is not a list instance. It is the runtime
    # types.GenericAlias object list[first.Model | second.Model], which follows a different display
    # path when super() rejects it as its first argument. This checks that the same-named classes
    # nested inside that runtime alias are distinguished as well.
    # error: [invalid-super-argument] "`types.GenericAlias` instance `list[first.Model | second.Model]` is not a valid class"
    super(list[first.Model | second.Model], [])

BoundedModel = TypeVar("BoundedModel", bound=second.Model)

def invalid_bounded_super(value: BoundedModel) -> None:
    super(first.Model, value)  # snapshot: invalid-super-argument
```

```snapshot
error[invalid-super-argument]: `BoundedModel@invalid_bounded_super` is not an instance or subclass of `<class 'first.Model'>` in `super(<class 'first.Model'>, BoundedModel@invalid_bounded_super)` call
  --> src/mdtest_snippet.py:26:5
   |
26 |     super(first.Model, value)  # snapshot: invalid-super-argument
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^
info: Type variable `BoundedModel` has upper bound `second.Model`
info: `second.Model` is not an instance or subclass of `<class 'first.Model'>`
```

## Dictionary subscript assignments

Dictionary key and value annotations identify both sides of an invalid assignment.

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

def assign_value(values: dict[int, first.Model], value: second.Model) -> None:
    values[0] = value  # snapshot: invalid-assignment

def assign_key(values: dict[first.Model, int], key: second.Model) -> None:
    values[key] = 0  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Invalid subscript assignment with key of type `Literal[0]` and value of type `second.Model` on object of type `dict[int, first.Model]`
 --> src/mdtest_snippet.py:5:5
  |
5 |     values[0] = value  # snapshot: invalid-assignment
  |     ^^^^^^^^^^^^-----
  |                 |
  |                 Expected value of type `first.Model`, got `second.Model`


error[invalid-assignment]: Invalid subscript assignment with key of type `second.Model` and value of type `Literal[0]` on object of type `dict[first.Model, int]`
 --> src/mdtest_snippet.py:8:5
  |
8 |     values[key] = 0  # snapshot: invalid-assignment
  |     ^^^^^^^---^^^^^
  |            |
  |            Expected key of type `first.Model`, got `second.Model`
```

## Subscript operations on same-named union members

Subscript assignment and deletion identify the union member that lacks the corresponding method.

`first.py`:

```py
class Container: ...
```

`second.py`:

```py
class Container:
    def __setitem__(self, key: int, value: int) -> None: ...
    def __delitem__(self, key: int) -> None: ...
```

```py
import first
import second

def assignment(value: first.Container | second.Container) -> None:
    value[0] = 1  # snapshot: invalid-assignment

def deletion(value: first.Container | second.Container) -> None:
    # error: [not-subscriptable] "Cannot delete subscript on object of type `first.Container` with no `__delitem__` method"
    del value[0]
```

```snapshot
error[invalid-assignment]: Cannot assign to a subscript on an object of type `first.Container`
 --> src/mdtest_snippet.py:5:5
  |
5 |     value[0] = 1  # snapshot: invalid-assignment
  |     ^^^^^^^^
info: The full type of the subscripted object is `first.Container | second.Container`
help: Consider adding a `__setitem__` method to `first.Container`.
```

## Identifying union members

Diagnostics identify the particular union member that lacks an attribute, context-manager method, or
valid boolean conversion.

`first.py`:

```py
class Model:
    present: int

class Context:
    def __enter__(self) -> None: ...
    def __exit__(self, *args: object) -> None: ...

class Bool:
    def __bool__(self) -> bool:
        return True
```

`second.py`:

```py
class Model: ...
class Context: ...

class Bool:
    def __bool__(self) -> int:
        return 1
```

```py
import first
import second

def missing_attribute(value: first.Model | second.Model) -> int:
    # error: [unresolved-attribute] "Attribute `present` is not defined on `second.Model` in union `first.Model | second.Model`"
    return value.present

def context_manager(value: first.Context | second.Context) -> None:
    with value:  # snapshot: invalid-context-manager
        pass

def boolean(value: first.Bool | second.Bool) -> None:
    # error: [unsupported-bool-conversion] "union `first.Bool | second.Bool` because `second.Bool` doesn't implement `__bool__` correctly"
    if value:
        pass
```

```snapshot
error[invalid-context-manager]: Object of type `first.Context | second.Context` cannot be used with `with` because the methods `__enter__` and `__exit__` are possibly missing
 --> src/mdtest_snippet.py:9:10
  |
9 |     with value:  # snapshot: invalid-context-manager
  |          ^^^^^
info: `second.Context` does not implement `__enter__` or `__exit__`
```

## Async context managers with same-named return types

Async context-manager diagnostics distinguish the manager from a non-awaitable return type.

`first.py`:

```py
import second

class Context:
    def __aenter__(self) -> second.Context:
        return second.Context()

    async def __aexit__(self, *args: object) -> None: ...
```

`second.py`:

```py
class Context: ...
```

```py
import first

async def use(value: first.Context) -> None:
    async with value:  # snapshot: invalid-context-manager
        pass
```

```snapshot
error[invalid-context-manager]: Object of type `first.Context` cannot be used with `async with` because `__aenter__` does not return an awaitable
 --> src/mdtest_snippet.py:4:16
  |
4 |     async with value:  # snapshot: invalid-context-manager
  |                ^^^^^
info: `__aenter__` returns `second.Context`, which is not awaitable
info: Consider declaring the method with `async def`
```

## Invalid iteration results

Iteration diagnostics distinguish an iterable from its same-named invalid iterator.

`first.py`:

```py
import second

class Iterable:
    def __iter__(self) -> second.Iterable:
        return second.Iterable()

class AsyncIterable:
    def __aiter__(self) -> second.AsyncIterable:
        return second.AsyncIterable()
```

`second.py`:

```py
class Iterable: ...
class AsyncIterable: ...
```

```py
import first

def consume(value: first.Iterable) -> None:
    for item in value:  # snapshot: not-iterable
        pass

async def consume_async(value: first.AsyncIterable) -> None:
    async for item in value:  # snapshot: not-iterable
        pass
```

```snapshot
error[not-iterable]: Object of type `first.Iterable` is not iterable
 --> src/mdtest_snippet.py:4:17
  |
4 |     for item in value:  # snapshot: not-iterable
  |                 ^^^^^
info: Its `__iter__` method returns an object of type `second.Iterable`, which has no `__next__` method


error[not-iterable]: Object of type `first.AsyncIterable` is not async-iterable
 --> src/mdtest_snippet.py:8:23
  |
8 |     async for item in value:  # snapshot: not-iterable
  |                       ^^^^^
info: Its `__aiter__` method returns an object of type `second.AsyncIterable`, which has no `__anext__` method
```

## Class inheritance conflicts

Diagnostics for MRO failures, conflicting metaclasses, and incompatible instance layouts distinguish
same-named classes across every displayed base.

`first.py`:

```py
class First: ...
class Second: ...
class Conflict(First, Second): ...
class Meta(type): ...
class Custom(metaclass=Meta): ...

class Layout:
    __slots__ = ("first",)
```

`second.py`:

```py
import first

class Conflict(first.Second, first.First): ...
class Meta(type): ...
class Custom(metaclass=Meta): ...

class Layout:
    __slots__ = ("second",)
```

```py
import first
import second

# error: [inconsistent-mro] "bases list `[<class 'first.Conflict'>, <class 'second.Conflict'>]`"
class BadMro(first.Conflict, second.Conflict): ...

# error: [inconsistent-mro] "bases `[<class 'first.Conflict'>, <class 'second.Conflict'>]`"
BadDynamicMro = type("BadDynamicMro", (first.Conflict, second.Conflict), {})

# error: [conflicting-metaclass] "`first.Meta` (metaclass of base class `first.Custom`) and `second.Meta` (metaclass of base class `second.Custom`)"
class BadMetaclass(first.Custom, second.Custom): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`BadDynamicMetaclass`) must be a subclass of the metaclasses of all its bases, but `first.Meta` (metaclass of base class `first.Custom`) and `second.Meta` (metaclass of base class `second.Custom`) have no subclass relationship"
BadDynamicMetaclass = type("BadDynamicMetaclass", (first.Custom, second.Custom), {})

# error: [instance-layout-conflict] "Bases `first.Layout` and `second.Layout` cannot be combined in multiple inheritance"
class BadLayout(first.Layout, second.Layout): ...
```

## Enum member value explanations

The expected and actual enum-member value types remain distinct in explanatory notes.

`first.py`:

```py
class Model: ...
```

`second.py`:

```py
class Model: ...
```

```py
from enum import Enum

import first
import second

class InvalidEnum(Enum):
    _value_: first.Model
    ITEM = second.Model()  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Enum member `ITEM` value is not assignable to expected type
 --> src/mdtest_snippet.py:8:5
  |
8 |     ITEM = second.Model()  # snapshot: invalid-assignment
  |     ^^^^
info: Expected `first.Model`, got `second.Model`
```
