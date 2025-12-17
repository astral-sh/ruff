# Dataclasses

## Basic

Decorating a class with `@dataclass` is a convenient way to add special methods such as `__init__`,
`__repr__`, and `__eq__` to a class. The following example shows the basic usage of the `@dataclass`
decorator. By default, only the three mentioned methods are generated.

```py
from dataclasses import dataclass

@dataclass
class Person:
    name: str
    age: int | None = None

alice1 = Person("Alice", 30)
alice2 = Person(name="Alice", age=30)
alice3 = Person(age=30, name="Alice")
alice4 = Person("Alice", age=30)

reveal_type(alice1)  # revealed: Person
reveal_type(type(alice1))  # revealed: type[Person]

reveal_type(alice1.name)  # revealed: str
reveal_type(alice1.age)  # revealed: int | None

reveal_type(repr(alice1))  # revealed: str

reveal_type(alice1 == alice2)  # revealed: bool
reveal_type(alice1 == "Alice")  # revealed: bool

bob = Person("Bob")
bob2 = Person("Bob", None)
bob3 = Person(name="Bob")
bob4 = Person(name="Bob", age=None)
```

The signature of the `__init__` method is generated based on the classes attributes. The following
calls are not valid:

```py
# error: [missing-argument]
Person()

# error: [too-many-positional-arguments]
Person("Eve", 20, "too many arguments")

# error: [invalid-argument-type]
Person("Eve", "string instead of int")

# error: [invalid-argument-type]
# error: [invalid-argument-type]
Person(20, "Eve")
```

## Signature of `__init__`

Declarations in the class body are used to generate the signature of the `__init__` method. If the
attributes are not just declarations, but also bindings, the type inferred from bindings is used as
the default value.

```py
from dataclasses import dataclass

@dataclass
class D:
    x: int
    y: str = "default"
    z: int | None = 1 + 2

reveal_type(D.__init__)  # revealed: (self: D, x: int, y: str = "default", z: int | None = 3) -> None
```

This also works if the declaration and binding are split:

```py
@dataclass
class D:
    x: int | None
    x = None

reveal_type(D.__init__)  # revealed: (self: D, x: int | None = None) -> None
```

Non-fully static types are handled correctly:

```py
from typing import Any

@dataclass
class C:
    w: type[Any]
    x: Any
    y: int | Any
    z: tuple[int, Any]

reveal_type(C.__init__)  # revealed: (self: C, w: type[Any], x: Any, y: int | Any, z: tuple[int, Any]) -> None
```

Variables without annotations are ignored:

```py
@dataclass
class D:
    x: int
    y = 1

reveal_type(D.__init__)  # revealed: (self: D, x: int) -> None
```

If attributes without default values are declared after attributes with default values, a
`TypeError` will be raised at runtime. Ideally, we would emit a diagnostic in that case:

```py
@dataclass
class D:
    x: int = 1
    # TODO: this should be an error: field without default defined after field with default
    y: str
```

Pure class attributes (`ClassVar`) are not included in the signature of `__init__`:

```py
from typing import ClassVar

@dataclass
class D:
    x: int
    y: ClassVar[str] = "default"
    z: bool

reveal_type(D.__init__)  # revealed: (self: D, x: int, z: bool) -> None

d = D(1, True)
reveal_type(d.x)  # revealed: int
reveal_type(d.y)  # revealed: str
reveal_type(d.z)  # revealed: bool
```

Function declarations do not affect the signature of `__init__`:

```py
@dataclass
class D:
    x: int

    def y(self) -> str:
        return ""

reveal_type(D.__init__)  # revealed: (self: D, x: int) -> None
```

And neither do nested class declarations:

```py
@dataclass
class D:
    x: int

    class Nested:
        y: str

reveal_type(D.__init__)  # revealed: (self: D, x: int) -> None
```

But if there is a variable annotation with a function or class literal type, the signature of
`__init__` will include this field:

```py
from ty_extensions import TypeOf

class SomeClass: ...

def some_function() -> None: ...
@dataclass
class D:
    function_literal: TypeOf[some_function]
    class_literal: TypeOf[SomeClass]
    class_subtype_of: type[SomeClass]

# revealed: (self: D, function_literal: def some_function() -> None, class_literal: <class 'SomeClass'>, class_subtype_of: type[SomeClass]) -> None
reveal_type(D.__init__)
```

More realistically, dataclasses can have `Callable` attributes:

```py
from typing import Callable

@dataclass
class D:
    c: Callable[[int], str]

reveal_type(D.__init__)  # revealed: (self: D, c: (int, /) -> str) -> None
```

Implicit instance attributes do not affect the signature of `__init__`:

```py
@dataclass
class D:
    x: int

    def f(self, y: str) -> None:
        self.y: str = y

reveal_type(D(1).y)  # revealed: str

reveal_type(D.__init__)  # revealed: (self: D, x: int) -> None
```

Annotating expressions does not lead to an entry in `__annotations__` at runtime, and so it wouldn't
be included in the signature of `__init__`. This is a case that we currently don't detect:

```py
@dataclass
class D:
    # (x) is an expression, not a "simple name"
    (x): int = 1

# TODO: should ideally not include a `x` parameter
reveal_type(D.__init__)  # revealed:(self: D, x: int = 1) -> None
```

## `@dataclass` calls with arguments

The `@dataclass` decorator can take several arguments to customize the existence of the generated
methods. The following test makes sure that we still treat the class as a dataclass if (the default)
arguments are passed in:

```py
from dataclasses import dataclass

@dataclass(init=True, repr=True, eq=True)
class Person:
    name: str
    age: int | None = None

alice = Person("Alice", 30)
reveal_type(repr(alice))  # revealed: str
reveal_type(alice == alice)  # revealed: bool
```

If `init` is set to `False`, no `__init__` method is generated:

```py
from dataclasses import dataclass

@dataclass(init=False)
class C:
    x: int

C()  # Okay

# error: [too-many-positional-arguments]
C(1)

repr(C())

C() == C()
```

## Other dataclass parameters

### `repr`

A custom `__repr__` method is generated by default. It can be disabled by passing `repr=False`, but
in that case `__repr__` is still available via `object.__repr__`:

```py
from dataclasses import dataclass

@dataclass(repr=False)
class WithoutRepr:
    x: int

reveal_type(WithoutRepr(1).__repr__)  # revealed: bound method WithoutRepr.__repr__() -> str
```

### `eq`

The same is true for `__eq__`. Setting `eq=False` disables the generated `__eq__` method, but
`__eq__` is still available via `object.__eq__`:

```py
from dataclasses import dataclass

@dataclass(eq=False)
class WithoutEq:
    x: int

reveal_type(WithoutEq(1) == WithoutEq(2))  # revealed: bool
```

### `order`

```toml
[environment]
python-version = "3.12"
```

`order` is set to `False` by default. If `order=True`, `__lt__`, `__le__`, `__gt__`, and `__ge__`
methods will be generated:

```py
from dataclasses import dataclass

@dataclass
class WithoutOrder:
    x: int

WithoutOrder(1) < WithoutOrder(2)  # error: [unsupported-operator]
WithoutOrder(1) <= WithoutOrder(2)  # error: [unsupported-operator]
WithoutOrder(1) > WithoutOrder(2)  # error: [unsupported-operator]
WithoutOrder(1) >= WithoutOrder(2)  # error: [unsupported-operator]

@dataclass(order=True)
class WithOrder:
    x: int

WithOrder(1) < WithOrder(2)
WithOrder(1) <= WithOrder(2)
WithOrder(1) > WithOrder(2)
WithOrder(1) >= WithOrder(2)
```

Comparisons are only allowed for `WithOrder` instances:

```py
WithOrder(1) < 2  # error: [unsupported-operator]
WithOrder(1) <= 2  # error: [unsupported-operator]
WithOrder(1) > 2  # error: [unsupported-operator]
WithOrder(1) >= 2  # error: [unsupported-operator]
```

This also works for generic dataclasses:

```py
from dataclasses import dataclass

@dataclass(order=True)
class GenericWithOrder[T]:
    x: T

GenericWithOrder[int](1) < GenericWithOrder[int](1)

GenericWithOrder[int](1) < GenericWithOrder[str]("a")  # error: [unsupported-operator]
```

If a class already defines one of the comparison methods, a `TypeError` is raised at runtime.
Ideally, we would emit a diagnostic in that case:

```py
@dataclass(order=True)
class AlreadyHasCustomDunderLt:
    x: int

    # TODO: Ideally, we would emit a diagnostic here
    def __lt__(self, other: object) -> bool:
        return False
```

### `__hash__` and `unsafe_hash`

If `eq` and `frozen` are both `True`, a `__hash__` method is generated by default:

```py
from dataclasses import dataclass

@dataclass(eq=True, frozen=True)
class WithHash:
    x: int

reveal_type(WithHash.__hash__)  # revealed: (self: WithHash) -> int
```

If `eq` is set to `True` and `frozen` is set to `False`, `__hash__` will be set to `None`, to mark
is unhashable (because it is mutable):

```py
from dataclasses import dataclass

@dataclass(eq=True, frozen=False)
class WithoutHash:
    x: int

reveal_type(WithoutHash.__hash__)  # revealed: None
```

If `eq` is set to `False`, `__hash__` will inherit from the parent class (which could be `object`).
Note that we see a revealed type of `def …` here, because `__hash__` refers to an actual function,
not a synthetic method like in the first example.

```py
from dataclasses import dataclass
from typing import Any

@dataclass(eq=False, frozen=False)
class InheritHash:
    x: int

reveal_type(InheritHash.__hash__)  # revealed: def __hash__(self) -> int

class Base:
    # Type the `self` parameter as `Any` to distinguish it from `object.__hash__`
    def __hash__(self: Any) -> int:
        return 42

@dataclass(eq=False, frozen=False)
class InheritHash(Base):
    x: int

reveal_type(InheritHash.__hash__)  # revealed: def __hash__(self: Any) -> int
```

If `unsafe_hash` is set to `True`, a `__hash__` method will be generated even if the dataclass is
mutable:

```py
from dataclasses import dataclass

@dataclass(eq=True, frozen=False, unsafe_hash=True)
class WithUnsafeHash:
    x: int

reveal_type(WithUnsafeHash.__hash__)  # revealed: (self: WithUnsafeHash) -> int
```

### `frozen`

If true (the default is False), assigning to fields will generate a diagnostic.

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class MyFrozenClass:
    x: int

frozen_instance = MyFrozenClass(1)
frozen_instance.x = 2  # error: [invalid-assignment]
```

If `__setattr__()` or `__delattr__()` is defined in the class, we should emit a diagnostic.

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class MyFrozenClass:
    x: int

    # TODO: Emit a diagnostic here
    def __setattr__(self, name: str, value: object) -> None: ...

    # TODO: Emit a diagnostic here
    def __delattr__(self, name: str) -> None: ...
```

This also works for generic dataclasses:

```toml
[environment]
python-version = "3.12"
```

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class MyFrozenGeneric[T]:
    x: T

frozen_instance = MyFrozenGeneric[int](1)
frozen_instance.x = 2  # error: [invalid-assignment]
```

Attempting to mutate an unresolved attribute on a frozen dataclass:

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class MyFrozenClass: ...

frozen = MyFrozenClass()
frozen.x = 2  # error: [invalid-assignment] "Cannot assign to unresolved attribute `x` on type `MyFrozenClass`"
```

A diagnostic is also emitted if a frozen dataclass is inherited, and an attempt is made to mutate an
attribute in the child class:

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class MyFrozenClass:
    x: int = 1

class MyFrozenChildClass(MyFrozenClass): ...

frozen = MyFrozenChildClass()
frozen.x = 2  # error: [invalid-assignment]
```

The same diagnostic is emitted if a frozen dataclass is inherited, and an attempt is made to delete
an attribute:

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class MyFrozenClass:
    x: int = 1

class MyFrozenChildClass(MyFrozenClass): ...

frozen = MyFrozenChildClass()
del frozen.x  # TODO this should emit an [invalid-assignment]
```

### frozen/non-frozen inheritance

If a non-frozen dataclass inherits from a frozen dataclass, an exception is raised at runtime. We
catch this error:

<!-- snapshot-diagnostics -->

`a.py`:

```py
from dataclasses import dataclass

@dataclass(frozen=True)
class FrozenBase:
    x: int

@dataclass
# error: [invalid-frozen-dataclass-subclass] "Non-frozen dataclass `Child` cannot inherit from frozen dataclass `FrozenBase`"
class Child(FrozenBase):
    y: int
```

Frozen dataclasses inheriting from non-frozen dataclasses are also illegal:

`b.py`:

```py
from dataclasses import dataclass

@dataclass
class Base:
    x: int

@dataclass(frozen=True)
# error: [invalid-frozen-dataclass-subclass] "Frozen dataclass `FrozenChild` cannot inherit from non-frozen dataclass `Base`"
class FrozenChild(Base):
    y: int
```

Example of diagnostics when there are multiple files involved:

`module.py`:

```py
import dataclasses

@dataclasses.dataclass(frozen=False)
class NotFrozenBase:
    x: int
```

`main.py`:

```py
from functools import total_ordering
from typing import final
from dataclasses import dataclass

from module import NotFrozenBase

@final
@dataclass(frozen=True)
@total_ordering
class FrozenChild(NotFrozenBase):  # error: [invalid-frozen-dataclass-subclass]
    y: str
```

### `match_args`

If `match_args` is set to `True` (the default), the `__match_args__` attribute is a tuple created
from the list of non keyword-only parameters to the synthesized `__init__` method (even if
`__init__` is not actually generated).

```py
from dataclasses import dataclass, field

@dataclass
class WithMatchArgs:
    normal_a: str
    normal_b: int
    kw_only: int = field(kw_only=True)

reveal_type(WithMatchArgs.__match_args__)  # revealed: tuple[Literal["normal_a"], Literal["normal_b"]]

@dataclass(kw_only=True)
class KwOnlyDefaultMatchArgs:
    normal_a: str = field(kw_only=False)
    normal_b: int = field(kw_only=False)
    kw_only: int

reveal_type(KwOnlyDefaultMatchArgs.__match_args__)  # revealed: tuple[Literal["normal_a"], Literal["normal_b"]]

@dataclass(match_args=True)
class ExplicitMatchArgs:
    normal: str

reveal_type(ExplicitMatchArgs.__match_args__)  # revealed: tuple[Literal["normal"]]

@dataclass
class Empty: ...

reveal_type(Empty.__match_args__)  # revealed: tuple[()]
```

When `match_args` is explicitly set to `False`, the `__match_args__` attribute is not available:

```py
@dataclass(match_args=False)
class NoMatchArgs:
    x: int
    y: str

NoMatchArgs.__match_args__  # error: [unresolved-attribute]
```

### `kw_only`

An error is emitted if a dataclass is defined with `kw_only=True` and positional arguments are
passed to the constructor.

```toml
[environment]
python-version = "3.10"
```

```py
from dataclasses import dataclass

@dataclass(kw_only=True)
class A:
    x: int
    y: int

# error: [missing-argument] "No arguments provided for required parameters `x`, `y`"
# error: [too-many-positional-arguments] "Too many positional arguments: expected 0, got 2"
a = A(1, 2)
a = A(x=1, y=2)
```

The class-level parameter can be overridden per-field.

```py
from dataclasses import dataclass, field

@dataclass(kw_only=True)
class A:
    a: str = field(kw_only=False)
    b: int = 0

reveal_type(A.__init__)  # revealed:(self: A, a: str, *, b: int = 0) -> None

A("hi")
```

If some fields are `kw_only`, they should appear after all positional fields in the `__init__`
signature.

```py
@dataclass
class A:
    b: int = field(kw_only=True, default=3)
    a: str

A("hi")
```

The field-level `kw_only` value takes precedence over the `KW_ONLY` pseudo-type.

```py
from dataclasses import field, dataclass, KW_ONLY

@dataclass
class C:
    _: KW_ONLY
    x: int = field(kw_only=False)

C(x=1)
C(1)
```

### `kw_only` - Python < 3.10

For Python < 3.10, `kw_only` is not supported.

```toml
[environment]
python-version = "3.9"
```

```py
from dataclasses import dataclass

@dataclass(kw_only=True)  # TODO: Emit a diagnostic here
class A:
    x: int
    y: int
```

### `kw_only` - Python 3.13

```toml
[environment]
python-version = "3.13"
```

```py
from dataclasses import dataclass, field

@dataclass
class Employee:
    e_id: int = field(kw_only=True, default=0)
    name: str

Employee("Alice")
Employee(name="Alice")
Employee(name="Alice", e_id=1)
Employee(e_id=1, name="Alice")
Employee("Alice", e_id=1)

Employee("Alice", 1)  # error: [too-many-positional-arguments]
```

### `kw_only` - Python 3.14

```toml
[environment]
python-version = "3.14"
```

```py
from dataclasses import dataclass, field

@dataclass
class Employee:
    # Python 3.14 introduces a new `doc` parameter for `dataclasses.field`
    e_id: int = field(kw_only=True, default=0, doc="Global employee ID")
    name: str

Employee("Alice")
Employee(name="Alice")
Employee(name="Alice", e_id=1)
Employee(e_id=1, name="Alice")
Employee("Alice", e_id=1)

Employee("Alice", 1)  # error: [too-many-positional-arguments]
```

### Inherited fields with class-level `kw_only`

When a child dataclass uses `@dataclass(kw_only=True)`, the `kw_only` setting should only apply to
fields defined in the child class, not to inherited fields from parent classes.

This is a regression test for <https://github.com/astral-sh/ty/issues/1769>.

```toml
[environment]
python-version = "3.10"
```

```py
from dataclasses import dataclass

@dataclass
class Inner:
    inner: int

@dataclass(kw_only=True)
class Outer(Inner):
    outer: int

# Inherited field `inner` is positional, new field `outer` is keyword-only
reveal_type(Outer.__init__)  # revealed: (self: Outer, inner: int, *, outer: int) -> None

Outer(0, outer=5)  # OK
Outer(inner=0, outer=5)  # Also OK
# error: [missing-argument]
# error: [too-many-positional-arguments]
Outer(0, 5)
```

This also works when the parent class uses the `KW_ONLY` sentinel:

```py
from dataclasses import dataclass, KW_ONLY

@dataclass
class Parent:
    a: int
    _: KW_ONLY
    b: str

@dataclass(kw_only=True)
class Child(Parent):
    c: bytes

# `a` is positional (from parent), `b` is keyword-only (from parent's KW_ONLY),
# `c` is keyword-only (from child's kw_only=True)
reveal_type(Child.__init__)  # revealed: (self: Child, a: int, *, b: str, c: bytes) -> None

Child(1, b="hello", c=b"world")  # OK
# error: [missing-argument] "No arguments provided for required parameters `b`, `c`"
# error: [too-many-positional-arguments]
Child(1, "hello", b"world")
```

And when the child class uses the `KW_ONLY` sentinel while inheriting from a parent:

```py
from dataclasses import dataclass, KW_ONLY

@dataclass
class Base:
    x: int

@dataclass
class Derived(Base):
    y: str
    _: KW_ONLY
    z: bytes

# `x` and `y` are positional, `z` is keyword-only (from Derived's KW_ONLY)
reveal_type(Derived.__init__)  # revealed: (self: Derived, x: int, y: str, *, z: bytes) -> None

Derived(1, "hello", z=b"world")  # OK
# error: [missing-argument]
# error: [too-many-positional-arguments]
Derived(1, "hello", b"world")
```

The reverse case also works: when a parent has `kw_only=True` but the child doesn't, the parent's
fields stay keyword-only while the child's fields are positional:

```py
from dataclasses import dataclass

@dataclass(kw_only=True)
class KwOnlyParent:
    parent_field: int

@dataclass
class PositionalChild(KwOnlyParent):
    child_field: str

# `child_field` is positional (child's default), `parent_field` stays keyword-only
reveal_type(PositionalChild.__init__)  # revealed: (self: PositionalChild, child_field: str, *, parent_field: int) -> None

PositionalChild("hello", parent_field=1)  # OK
# error: [missing-argument]
# error: [too-many-positional-arguments]
PositionalChild("hello", 1)
```

### `slots`

If a dataclass is defined with `slots=True`, the `__slots__` attribute is generated as a tuple. It
is not present otherwise.

```py
from dataclasses import dataclass
from typing import Tuple

@dataclass
class A:
    x: int
    y: int

# revealed: Unknown
# error: [unresolved-attribute]
reveal_type(A.__slots__)

@dataclass(slots=True)
class B:
    x: int
    y: int

reveal_type(B.__slots__)  # revealed: tuple[Literal["x"], Literal["y"]]
```

### `weakref_slot`

When a dataclass is defined with `weakref_slot=True` on Python >=3.11, the `__weakref__` attribute
is generated. For now, we do not attempt to infer a more precise type for it.

```toml
[environment]
python-version = "3.11"
```

```py
from dataclasses import dataclass

@dataclass(slots=True, weakref_slot=True)
class C:
    x: int

reveal_type(C.__weakref__)  # revealed: Any | None
```

The `__weakref__` attribute is correctly not modeled as existing on instances of slotted dataclasses
where the class definition was not marked with `weakref=True`:

```py
from dataclasses import dataclass

@dataclass(slots=True)
class C: ...

# error: [unresolved-attribute]
reveal_type(C().__weakref__)  # revealed: Unknown
```

### New features are not available on old Python versions

Certain parameters to `@dataclass` were added on newer Python versions; we do not infer them as
having any effect on older Python versions:

```toml
[environment]
python-version = "3.9"
```

```py
from dataclasses import dataclass

# fmt: off

# TODO: these nonexistent keyword arguments should cause us to emit diagnostics on Python 3.9
@dataclass(
    slots=True,
    weakref_slot=True,
    match_args=True
)
class Foo: ...

# fmt: on

# error: [unresolved-attribute]
reveal_type(Foo.__slots__)  # revealed: Unknown
# error: [unresolved-attribute]
reveal_type(Foo.__match_args__)  # revealed: Unknown

# TODO: this actually *does* exist at runtime
# (all classes and non-slotted instances have it available by default).
# We could try to model that more fully...?
# It's not added by the dataclasses machinery, though.
#
# error: [unresolved-attribute]
reveal_type(Foo.__weakref__)  # revealed: Unknown
```

## `Final` fields

Dataclass fields can be annotated with `Final`, which means that the field cannot be reassigned
after the instance is created. Fields that are additionally annotated with `ClassVar` are not part
of the `__init__` signature.

```py
from dataclasses import dataclass
from typing import Final, ClassVar

@dataclass
class C:
    # a `Final` annotation without a right-hand side is not allowed in normal classes,
    # but valid for dataclasses. The field will be initialized in the synthesized
    # `__init__` method
    instance_variable_no_default: Final[int]
    instance_variable: Final[int] = 1
    class_variable1: ClassVar[Final[int]] = 1
    class_variable2: ClassVar[Final[int]] = 1

reveal_type(C.__init__)  # revealed:(self: C, instance_variable_no_default: int, instance_variable: int = 1) -> None

c = C(1)
# error: [invalid-assignment] "Cannot assign to final attribute `instance_variable` on type `C`"
c.instance_variable = 2
```

## Inheritance

### Normal class inheriting from a dataclass

```py
from dataclasses import dataclass

@dataclass
class Base:
    x: int

class Derived(Base): ...

d = Derived(1)  # OK
reveal_type(d.x)  # revealed: int
```

### Dataclass inheriting from normal class

```py
from dataclasses import dataclass

class Base:
    x: int = 1

@dataclass
class Derived(Base):
    y: str

d = Derived("a")

# error: [too-many-positional-arguments]
# error: [invalid-argument-type]
Derived(1, "a")
```

### Dataclass inheriting from another dataclass

```py
from dataclasses import dataclass

@dataclass
class Base:
    x: int
    y: str

@dataclass
class Derived(Base):
    z: bool

d = Derived(1, "a", True)  # OK

reveal_type(d.x)  # revealed: int
reveal_type(d.y)  # revealed: str
reveal_type(d.z)  # revealed: bool

# error: [missing-argument]
Derived(1, "a")

# error: [missing-argument]
Derived(True)
```

### Overwriting attributes from base class

The following example comes from the
[Python documentation](https://docs.python.org/3/library/dataclasses.html#inheritance). The `x`
attribute appears just once in the `__init__` signature, and the default value is taken from the
derived class

```py
from dataclasses import dataclass
from typing import Any

@dataclass
class Base:
    x: Any = 15.0
    y: int = 0

@dataclass
class C(Base):
    z: int = 10
    x: int = 15

reveal_type(C.__init__)  # revealed:(self: C, x: int = 15, y: int = 0, z: int = 10) -> None
```

## Conditionally defined fields

### Statically known conditions

Fields that are defined in always-reachable branches are always present in the synthesized
`__init__` method. Fields that are defined in never-reachable branches are not present:

```py
from dataclasses import dataclass

@dataclass
class C:
    normal: int

    if 1 + 2 == 3:
        always_present: str

    if 1 + 2 == 4:
        never_present: bool

reveal_type(C.__init__)  # revealed: (self: C, normal: int, always_present: str) -> None
```

### Dynamic conditions

If a field is conditionally defined, we currently assume that it is always present. A more complex
alternative here would be to synthesized a union of all possible `__init__` signatures:

```py
from dataclasses import dataclass

def flag() -> bool:
    return True

@dataclass
class C:
    normal: int

    if flag():
        conditionally_present: str

reveal_type(C.__init__)  # revealed: (self: C, normal: int, conditionally_present: str) -> None
```

## Generic dataclasses

```toml
[environment]
python-version = "3.12"
```

### Basic

```py
from dataclasses import dataclass

@dataclass
class DataWithDescription[T]:
    data: T
    description: str

reveal_type(DataWithDescription[int])  # revealed: <class 'DataWithDescription[int]'>

d_int = DataWithDescription[int](1, "description")  # OK
reveal_type(d_int.data)  # revealed: int
reveal_type(d_int.description)  # revealed: str

# error: [invalid-argument-type]
DataWithDescription[int](None, "description")
```

### Deriving from generic dataclasses

This is a regression test for <https://github.com/astral-sh/ty/issues/853>.

```py
from dataclasses import dataclass

@dataclass
class Wrap[T]:
    data: T

reveal_type(Wrap[int].__init__)  # revealed: (self: Wrap[int], data: int) -> None

@dataclass
class WrappedInt(Wrap[int]):
    other_field: str

reveal_type(WrappedInt.__init__)  # revealed: (self: WrappedInt, data: int, other_field: str) -> None

# Make sure that another generic type parameter does not affect the `data` field
@dataclass
class WrappedIntAndExtraData[T](Wrap[int]):
    extra_data: T

# revealed: (self: WrappedIntAndExtraData[bytes], data: int, extra_data: bytes) -> None
reveal_type(WrappedIntAndExtraData[bytes].__init__)
```

### Non-dataclass inheriting from generic dataclass

This is a regression test for <https://github.com/astral-sh/ty/issues/1427>.

When a non-dataclass inherits from a generic dataclass, the generic type parameters should still be
properly inferred when calling the inherited `__init__` method.

```py
from dataclasses import dataclass

@dataclass
class ParentDataclass[T]:
    value: T

# Non-dataclass inheriting from generic dataclass
class ChildOfParentDataclass[T](ParentDataclass[T]): ...

def uses_dataclass[T](x: T) -> ChildOfParentDataclass[T]:
    return ChildOfParentDataclass(x)

# TODO: ParentDataclass.__init__ should show generic types, not Unknown
# revealed: (self: ParentDataclass[Unknown], value: Unknown) -> None
reveal_type(ParentDataclass.__init__)

# revealed: (self: ParentDataclass[T@ChildOfParentDataclass], value: T@ChildOfParentDataclass) -> None
reveal_type(ChildOfParentDataclass.__init__)

result_int = uses_dataclass(42)
reveal_type(result_int)  # revealed: ChildOfParentDataclass[int]

result_str = uses_dataclass("hello")
reveal_type(result_str)  # revealed: ChildOfParentDataclass[str]
```

## Descriptor-typed fields

### Same type in `__get__` and `__set__`

For the following descriptor, the return type of `__get__` and the type of the `value` parameter in
`__set__` are the same. The generated `__init__` method takes an argument of this type (instead of
the type of the descriptor), and the default value is also of this type:

```py
from typing import overload
from dataclasses import dataclass

class UppercaseString:
    _value: str = ""

    def __get__(self, instance: object, owner: None | type) -> str:
        return self._value

    def __set__(self, instance: object, value: str) -> None:
        self._value = value.upper()

@dataclass
class C:
    upper: UppercaseString = UppercaseString()

reveal_type(C.__init__)  # revealed: (self: C, upper: str = ...) -> None

c = C("abc")
reveal_type(c.upper)  # revealed: str

# This is also okay:
C()

# error: [invalid-argument-type]
C(1)

# error: [too-many-positional-arguments]
C("a", "b")
```

### Different types in `__get__` and `__set__`

In general, the type of the `__init__` parameter is determined by the `value` parameter type of the
`__set__` method (`str` in the example below). However, the default value is generated by calling
the descriptor's `__get__` method as if it had been called on the class itself, i.e. passing `None`
for the `instance` argument.

```py
from typing import Literal, overload
from dataclasses import dataclass

class ConvertToLength:
    _len: int = 0

    @overload
    def __get__(self, instance: None, owner: type) -> Literal[""]: ...
    @overload
    def __get__(self, instance: object, owner: type | None) -> int: ...
    def __get__(self, instance: object | None, owner: type | None) -> str | int:
        if instance is None:
            return ""

        return self._len

    def __set__(self, instance, value: str) -> None:
        self._len = len(value)

@dataclass
class C:
    converter: ConvertToLength = ConvertToLength()

reveal_type(C.__init__)  # revealed: (self: C, converter: str = "") -> None

c = C("abc")
reveal_type(c.converter)  # revealed: int

# This is also okay:
C()

# error: [invalid-argument-type]
C(1)

# error: [too-many-positional-arguments]
C("a", "b")
```

### With overloaded `__set__` method

If the `__set__` method is overloaded, we determine the type for the `__init__` parameter as the
union of all possible `value` parameter types:

```py
from typing import overload
from dataclasses import dataclass

class AcceptsStrAndInt:
    def __get__(self, instance, owner) -> int:
        return 0

    @overload
    def __set__(self, instance: object, value: str) -> None: ...
    @overload
    def __set__(self, instance: object, value: int) -> None: ...
    def __set__(self, instance: object, value) -> None:
        pass

@dataclass
class C:
    field: AcceptsStrAndInt = AcceptsStrAndInt()

reveal_type(C.__init__)  # revealed: (self: C, field: str | int = ...) -> None
```

## `dataclasses.field`

To do

## `dataclass.fields`

Dataclasses have a special `__dataclass_fields__` class variable member. The `DataclassInstance`
protocol checks for the presence of this attribute. It is used in the `dataclasses.fields` and
`dataclasses.asdict` functions, for example:

```py
from dataclasses import dataclass, fields, asdict

@dataclass
class Foo:
    x: int

foo = Foo(1)

reveal_type(foo.__dataclass_fields__)  # revealed: dict[str, Field[Any]]
reveal_type(fields(Foo))  # revealed: tuple[Field[Any], ...]
reveal_type(asdict(foo))  # revealed: dict[str, Any]
```

The class objects themselves also have a `__dataclass_fields__` attribute:

```py
reveal_type(Foo.__dataclass_fields__)  # revealed: dict[str, Field[Any]]
```

They can be passed into `fields` as well, because it also accepts `type[DataclassInstance]`
arguments:

```py
reveal_type(fields(Foo))  # revealed: tuple[Field[Any], ...]
```

But calling `asdict` on the class object is not allowed:

```py
# TODO: this should be a invalid-argument-type error, but we don't properly check the
# types (and more importantly, the `ClassVar` type qualifier) of protocol members yet.
asdict(Foo)
```

## `dataclasses.KW_ONLY`

<!-- snapshot-diagnostics -->

If an attribute is annotated with `dataclasses.KW_ONLY`, it is not added to the synthesized
`__init__` of the class. Instead, this special marker annotation causes Python at runtime to ensure
that all annotations following it have keyword-only parameters generated for them in the class's
synthesized `__init__` method.

```toml
[environment]
python-version = "3.10"
```

```py
from dataclasses import dataclass, field, KW_ONLY

@dataclass
class C:
    x: int
    _: KW_ONLY
    y: str

reveal_type(C.__init__)  # revealed: (self: C, x: int, *, y: str) -> None

# error: [missing-argument]
# error: [too-many-positional-arguments]
C(3, "")

C(3, y="")
```

Using `KW_ONLY` to annotate more than one field in a dataclass causes a `TypeError` to be raised at
runtime:

```py
@dataclass
class Fails:  # error: [duplicate-kw-only]
    a: int
    b: KW_ONLY
    c: str
    d: KW_ONLY
    e: bytes

reveal_type(Fails.__init__)  # revealed: (self: Fails, a: int, *, c: str, e: bytes) -> None
```

This also works if `KW_ONLY` is used in a conditional branch:

```py
def flag() -> bool:
    return True

@dataclass
class D:  # error: [duplicate-kw-only]
    x: int
    _1: KW_ONLY

    if flag():
        y: str
        _2: KW_ONLY
        z: float
```

`KW_ONLY` should only affect fields declared after it within the same class, not fields in
subclasses:

```py
from dataclasses import dataclass, KW_ONLY

@dataclass
class D:
    x: int
    _: KW_ONLY
    y: str

@dataclass
class E(D):
    z: bytes

# This should work: x=1 (positional), z=b"foo" (positional), y="foo" (keyword-only)
E(1, b"foo", y="foo")

reveal_type(E.__init__)  # revealed: (self: E, x: int, z: bytes, *, y: str) -> None
```

## Other special cases

### `dataclasses.dataclass`

We also understand dataclasses if they are decorated with the fully qualified name:

```py
import dataclasses

@dataclasses.dataclass
class C:
    x: str

reveal_type(C.__init__)  # revealed: (self: C, x: str) -> None
```

### Dataclass with custom `__init__` method

If a class already defines `__init__`, it is not replaced by the `dataclass` decorator.

```py
from dataclasses import dataclass

@dataclass(init=True)
class C:
    x: str

    def __init__(self, x: int) -> None:
        self.x = str(x)

C(1)  # OK

# error: [invalid-argument-type]
C("a")
```

Similarly, if we set `init=False`, we still recognize the custom `__init__` method:

```py
@dataclass(init=False)
class D:
    def __init__(self, x: int) -> None:
        self.x = str(x)

D(1)  # OK
D()  # error: [missing-argument]
```

### Return type of `dataclass(...)`

A call like `dataclass(order=True)` returns a callable itself, which is then used as the decorator.
We can store the callable in a variable and later use it as a decorator:

```py
from dataclasses import dataclass

dataclass_with_order = dataclass(order=True)

reveal_type(dataclass_with_order)  # revealed: <decorator produced by dataclass-like function>

@dataclass_with_order
class C:
    x: int

C(1) < C(2)  # ok
```

### Using `dataclass` as a function

```py
from dataclasses import dataclass

class B:
    x: int

# error: [missing-argument]
dataclass(B)()

# error: [invalid-argument-type]
dataclass(B)("a")

reveal_type(dataclass(B)(3).x)  # revealed: int
```

## Internals

The `dataclass` decorator returns the class itself. This means that the type of `Person` is `type`,
and attributes like the MRO are unchanged:

```py
from dataclasses import dataclass
from ty_extensions import reveal_mro

@dataclass
class Person:
    name: str
    age: int | None = None

reveal_type(type(Person))  # revealed: <class 'type'>
reveal_type(Person.__mro__)  # revealed: tuple[type, ...]
reveal_mro(Person)  # revealed: (<class 'Person'>, <class 'object'>)
```

The generated methods have the following signatures:

```py
reveal_type(Person.__init__)  # revealed: (self: Person, name: str, age: int | None = None) -> None

reveal_type(Person.__repr__)  # revealed: def __repr__(self) -> str

reveal_type(Person.__eq__)  # revealed: def __eq__(self, value: object, /) -> bool
```

## Function-like behavior of synthesized methods

Here, we make sure that the synthesized methods of dataclasses behave like proper functions.

```toml
[environment]
python-version = "3.12"
```

```py
from dataclasses import dataclass
from typing import Callable
from types import FunctionType
from ty_extensions import CallableTypeOf, TypeOf, static_assert, is_subtype_of, is_assignable_to, is_equivalent_to

@dataclass(order=True)
class C:
    x: int

reveal_type(C.__init__)  # revealed: (self: C, x: int) -> None
reveal_type(type(C.__init__))  # revealed: <class 'FunctionType'>

# We can access attributes that are defined on functions:
reveal_type(type(C.__init__).__code__)  # revealed: CodeType
reveal_type(C.__init__.__code__)  # revealed: CodeType

def equivalent_signature(self: C, x: int) -> None:
    pass

type DunderInitType = TypeOf[C.__init__]
type EquivalentPureCallableType = Callable[[C, int], None]
type EquivalentFunctionLikeCallableType = CallableTypeOf[equivalent_signature]

static_assert(is_subtype_of(DunderInitType, EquivalentPureCallableType))
static_assert(is_assignable_to(DunderInitType, EquivalentPureCallableType))

static_assert(not is_subtype_of(EquivalentPureCallableType, DunderInitType))
static_assert(not is_assignable_to(EquivalentPureCallableType, DunderInitType))

static_assert(is_subtype_of(DunderInitType, EquivalentFunctionLikeCallableType))
static_assert(is_assignable_to(DunderInitType, EquivalentFunctionLikeCallableType))

static_assert(is_subtype_of(EquivalentFunctionLikeCallableType, DunderInitType))
static_assert(is_assignable_to(EquivalentFunctionLikeCallableType, DunderInitType))

static_assert(is_equivalent_to(EquivalentFunctionLikeCallableType, DunderInitType))

static_assert(is_subtype_of(DunderInitType, FunctionType))
```

It should be possible to mock out synthesized methods:

```py
from unittest.mock import Mock

def test_c():
    c = C(1)
    c.__lt__ = Mock()
```

## Imperatively calling `dataclasses.dataclass`

While we do not currently recognize the special behaviour of `dataclasses.dataclass` if it is called
imperatively, we recognize that it can be called imperatively and do not emit any false-positive
diagnostics on such calls:

```py
from dataclasses import dataclass
from typing_extensions import TypeVar, dataclass_transform

U = TypeVar("U")

@dataclass_transform(kw_only_default=True)
def sequence(cls: type[U]) -> type[U]:
    d = dataclass(
        repr=False,
        eq=False,
        match_args=False,
        kw_only=True,
    )(cls)
    reveal_type(d)  # revealed: type[U@sequence] & Any
    return d

@dataclass_transform(kw_only_default=True)
def sequence2(cls: type) -> type:
    d = dataclass(
        repr=False,
        eq=False,
        match_args=False,
        kw_only=True,
    )(cls)
    reveal_type(d)  # revealed: type & Any
    return d

@dataclass_transform(kw_only_default=True)
def sequence3(cls: type[U]) -> type[U]:
    # TODO: should reveal `type[U@sequence3]`
    return reveal_type(dataclass(cls))  # revealed: Unknown

@dataclass_transform(kw_only_default=True)
def sequence4(cls: type) -> type:
    # TODO: should reveal `type`
    return reveal_type(dataclass(cls))  # revealed: Unknown

class Foo: ...

ordered_foo = dataclass(order=True)(Foo)
reveal_type(ordered_foo)  # revealed: type[Foo] & Any
# TODO: should be `Foo & Any`
reveal_type(ordered_foo())  # revealed: @Todo(Type::Intersection.call)
# TODO: should be `Any`
reveal_type(ordered_foo() < ordered_foo())  # revealed: @Todo(Type::Intersection.call)
```
