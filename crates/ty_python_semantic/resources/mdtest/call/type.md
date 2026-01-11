# Calls to `type()`

## Single-argument form

A single-argument call to `type()` returns an object that has the argument's meta-type. (This is
tested more extensively in `crates/ty_python_semantic/resources/mdtest/attributes.md`, alongside the
tests for the `__class__` attribute.)

```py
reveal_type(type(1))  # revealed: <class 'int'>
```

## Three-argument form (dynamic class creation)

A three-argument call to `type()` creates a new class. We synthesize a class type using the name
from the first argument:

```py
class Base: ...
class Mixin: ...

# We synthesize a class type using the name argument
reveal_type(type("Foo", (), {}))  # revealed: <class 'Foo'>

# With a single base class
reveal_type(type("Foo", (Base,), {"attr": 1}))  # revealed: <class 'Foo'>

# With multiple base classes
reveal_type(type("Foo", (Base, Mixin), {}))  # revealed: <class 'Foo'>

# The inferred type is assignable to type[Base] since Foo inherits from Base
tests: list[type[Base]] = []
testCaseClass = type("Foo", (Base,), {})
tests.append(testCaseClass)  # No error - type[Foo] is assignable to type[Base]
```

The name can also be provided indirectly via a variable with a string literal type:

```py
name = "IndirectClass"
IndirectClass = type(name, (), {})
reveal_type(IndirectClass)  # revealed: <class 'IndirectClass'>

# Works with base classes too
class Base: ...

base_name = "DerivedClass"
DerivedClass = type(base_name, (Base,), {})
reveal_type(DerivedClass)  # revealed: <class 'DerivedClass'>
```

## Distinct class types

Each `type()` call produces a distinct class type, even if they have the same name and bases:

```py
from ty_extensions import static_assert, is_equivalent_to

class Base: ...

Foo1 = type("Foo", (Base,), {})
Foo2 = type("Foo", (Base,), {})

# Even though they have the same name and bases, they are distinct types
static_assert(not is_equivalent_to(Foo1, Foo2))

# Each instance is typed with its respective class
foo1 = Foo1()
foo2 = Foo2()

def takes_foo1(x: Foo1) -> None: ...
def takes_foo2(x: Foo2) -> None: ...

takes_foo1(foo1)  # OK
takes_foo2(foo2)  # OK

# error: [invalid-argument-type] "Argument to function `takes_foo1` is incorrect: Expected `mdtest_snippet.Foo @ src/mdtest_snippet.py:5`, found `mdtest_snippet.Foo @ src/mdtest_snippet.py:6`"
takes_foo1(foo2)
# error: [invalid-argument-type] "Argument to function `takes_foo2` is incorrect: Expected `mdtest_snippet.Foo @ src/mdtest_snippet.py:6`, found `mdtest_snippet.Foo @ src/mdtest_snippet.py:5`"
takes_foo2(foo1)
```

## Instances and attribute access

Instances of dynamic classes are typed with the synthesized class name. Attributes from all base
classes are accessible:

```py
class Base:
    base_attr: int = 1

    def base_method(self) -> str:
        return "hello"

class Mixin:
    mixin_attr: str = "mixin"

Foo = type("Foo", (Base,), {})
foo = Foo()

# Instance is typed with the synthesized class name
reveal_type(foo)  # revealed: Foo

# Inherited attributes are accessible
reveal_type(foo.base_attr)  # revealed: int
reveal_type(foo.base_method())  # revealed: str

# Multiple inheritance: attributes from all bases are accessible
Bar = type("Bar", (Base, Mixin), {})
bar = Bar()
reveal_type(bar.base_attr)  # revealed: int
reveal_type(bar.mixin_attr)  # revealed: str
```

Attributes from the namespace dict (third argument) are not tracked. Like Pyright, we error when
attempting to access them:

```py
class Base: ...

Foo = type("Foo", (Base,), {"custom_attr": 42})
foo = Foo()

# error: [unresolved-attribute] "Object of type `Foo` has no attribute `custom_attr`"
reveal_type(foo.custom_attr)  # revealed: Unknown
```

## Inheritance from dynamic classes

Regular classes can inherit from dynamic classes:

```py
class Base:
    base_attr: int = 1

DynamicClass = type("DynamicClass", (Base,), {})

class Child(DynamicClass):
    child_attr: str = "child"

child = Child()

# Attributes from the dynamic class's base are accessible
reveal_type(child.base_attr)  # revealed: int

# The child class's own attributes are accessible
reveal_type(child.child_attr)  # revealed: str

# Child instances are subtypes of DynamicClass instances
def takes_dynamic(x: DynamicClass) -> None: ...

takes_dynamic(child)  # No error - Child is a subtype of DynamicClass

# isinstance narrows to the dynamic class instance type
def check_isinstance(x: object) -> None:
    if isinstance(x, DynamicClass):
        reveal_type(x)  # revealed: DynamicClass

# Dynamic class inheriting from int narrows correctly with isinstance
IntSubclass = type("IntSubclass", (int,), {})

def check_int_subclass(x: IntSubclass | str) -> None:
    if isinstance(x, int):
        # IntSubclass inherits from int, so it's included in the narrowed type
        reveal_type(x)  # revealed: IntSubclass
    else:
        reveal_type(x)  # revealed: str
```

## Disjointness

Dynamic classes are not considered disjoint from unrelated types (since a subclass could inherit
from both):

```py
class Base: ...

Foo = type("Foo", (Base,), {})

def check_disjointness(x: Foo | int) -> None:
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    else:
        # Foo and int are not considered disjoint because `class C(Foo, int)` could exist.
        reveal_type(x)  # revealed: Foo & ~int
```

Disjointness also works for `type[]` of dynamic classes:

```py
from ty_extensions import is_disjoint_from, static_assert

# Dynamic classes with disjoint bases have disjoint type[] types.
IntClass = type("IntClass", (int,), {})
StrClass = type("StrClass", (str,), {})

static_assert(is_disjoint_from(type[IntClass], type[StrClass]))
static_assert(is_disjoint_from(type[StrClass], type[IntClass]))

# Dynamic classes that share a common base are not disjoint.
class Base: ...

Foo = type("Foo", (Base,), {})
Bar = type("Bar", (Base,), {})

static_assert(not is_disjoint_from(type[Foo], type[Bar]))
```

## Using dynamic classes with `super()`

Dynamic classes can be used as the pivot class in `super()` calls:

```py
class Base:
    def method(self) -> int:
        return 42

DynamicChild = type("DynamicChild", (Base,), {})

# Using dynamic class as pivot with dynamic class instance owner
fc = DynamicChild()
reveal_type(super(DynamicChild, fc))  # revealed: <super: <class 'DynamicChild'>, DynamicChild>
reveal_type(super(DynamicChild, fc).method())  # revealed: int

# Regular class inheriting from dynamic class
class RegularChild(DynamicChild):
    pass

rc = RegularChild()
reveal_type(super(RegularChild, rc))  # revealed: <super: <class 'RegularChild'>, RegularChild>
reveal_type(super(RegularChild, rc).method())  # revealed: int

# Using dynamic class as pivot with regular class instance owner
reveal_type(super(DynamicChild, rc))  # revealed: <super: <class 'DynamicChild'>, RegularChild>
reveal_type(super(DynamicChild, rc).method())  # revealed: int
```

## Dynamic class inheritance chains

Dynamic classes can inherit from other dynamic classes:

```py
class Base:
    base_attr: int = 1

# Create a dynamic class that inherits from a regular class.
Parent = type("Parent", (Base,), {})
reveal_type(Parent)  # revealed: <class 'Parent'>

# Create a dynamic class that inherits from another dynamic class.
ChildCls = type("ChildCls", (Parent,), {})
reveal_type(ChildCls)  # revealed: <class 'ChildCls'>

# Child instances have access to attributes from the entire inheritance chain.
child = ChildCls()
reveal_type(child)  # revealed: ChildCls
reveal_type(child.base_attr)  # revealed: int

# Child instances are subtypes of `Parent` instances.
def takes_parent(x: Parent) -> None: ...

takes_parent(child)  # No error - `ChildCls` is a subtype of `Parent`
```

## Dataclass transform inheritance

Dynamic classes that inherit from a `@dataclass_transform()` decorated base class are recognized as
dataclass-like and have the synthesized `__dataclass_fields__` attribute:

```py
from dataclasses import Field
from typing_extensions import dataclass_transform

@dataclass_transform()
class DataclassBase:
    """Base class decorated with @dataclass_transform()."""

    pass

# A dynamic class inheriting from a dataclass_transform base
DynamicModel = type("DynamicModel", (DataclassBase,), {})

# The dynamic class has __dataclass_fields__ synthesized
reveal_type(DynamicModel.__dataclass_fields__)  # revealed: dict[str, Field[Any]]
```

## Applying `@dataclass` decorator directly

Applying the `@dataclass` decorator directly to a dynamic class is supported:

```py
from dataclasses import dataclass

Foo = type("Foo", (), {})
Foo = dataclass(Foo)

reveal_type(Foo.__dataclass_fields__)  # revealed: dict[str, Field[Any]]
```

## Generic base classes

Dynamic classes with generic base classes:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Container(Generic[T]):
    value: T

# Dynamic class inheriting from a generic class specialization
IntContainer = type("IntContainer", (Container[int],), {})
reveal_type(IntContainer)  # revealed: <class 'IntContainer'>

container = IntContainer()
reveal_type(container)  # revealed: IntContainer
reveal_type(container.value)  # revealed: int
```

## `type()` and `__class__` on dynamic instances

`type(instance)` returns the class of the dynamic instance:

```py
class Base: ...

Foo = type("Foo", (Base,), {})
foo = Foo()

# type() on an instance returns the class
reveal_type(type(foo))  # revealed: type[Foo]
```

`__class__` attribute access on dynamic instances:

```py
class Base: ...

Foo = type("Foo", (Base,), {})
foo = Foo()

# __class__ returns the class type
reveal_type(foo.__class__)  # revealed: type[Foo]
```

`__class__` on the dynamic class itself returns the metaclass (consistent with static classes):

```py
class StaticClass: ...

DynamicClass = type("DynamicClass", (), {})

# Both static and dynamic classes have `type` as their metaclass
reveal_type(StaticClass.__class__)  # revealed: <class 'type'>
reveal_type(DynamicClass.__class__)  # revealed: <class 'type'>
```

## Subtype relationships

Dynamic instances are subtypes of `object`:

```py
class Base: ...

Foo = type("Foo", (Base,), {})
foo = Foo()

# All dynamic instances are subtypes of object
def takes_object(x: object) -> None: ...

takes_object(foo)  # No error - Foo is a subtype of object

# Even dynamic classes with no explicit bases are subtypes of object
EmptyBases = type("EmptyBases", (), {})
empty = EmptyBases()
takes_object(empty)  # No error
```

## Attributes from `builtins.type`

Attributes defined on `builtins.type` are accessible on dynamic classes:

```py
T = type("T", (), {})

# Inherited from `builtins.type`:
reveal_type(T.__dictoffset__)  # revealed: int
reveal_type(T.__name__)  # revealed: str
reveal_type(T.__bases__)  # revealed: tuple[type, ...]
reveal_type(T.__mro__)  # revealed: tuple[type, ...]
```

## Invalid calls

Other numbers of arguments are invalid:

```py
# error: [no-matching-overload] "No overload of class `type` matches arguments"
reveal_type(type("Foo", ()))  # revealed: Unknown

# TODO: the keyword arguments for `Foo`/`Bar`/`Baz` here are invalid
# (you cannot pass `metaclass=` to `type()`, and none of them have
# base classes with `__init_subclass__` methods),
# but `type[Unknown]` would be better than `Unknown` here
#
# error: [no-matching-overload] "No overload of class `type` matches arguments"
reveal_type(type("Foo", (), {}, weird_other_arg=42))  # revealed: Unknown
# error: [no-matching-overload] "No overload of class `type` matches arguments"
reveal_type(type("Bar", (int,), {}, weird_other_arg=42))  # revealed: Unknown
# error: [no-matching-overload] "No overload of class `type` matches arguments"
reveal_type(type("Baz", (), {}, metaclass=type))  # revealed: Unknown
```

The following calls are also invalid, due to incorrect argument types:

```py
class Base: ...

# error: [invalid-argument-type] "Invalid argument to parameter 1 (`name`) of `type()`: Expected `str`, found `Literal[b"Foo"]`"
type(b"Foo", (), {})

# error: [invalid-argument-type] "Invalid argument to parameter 2 (`bases`) of `type()`: Expected `tuple[type, ...]`, found `<class 'Base'>`"
type("Foo", Base, {})

# error: 14 [invalid-base] "Invalid class base with type `Literal[1]`"
# error: 17 [invalid-base] "Invalid class base with type `Literal[2]`"
type("Foo", (1, 2), {})

# error: [invalid-argument-type] "Invalid argument to parameter 3 (`namespace`) of `type()`: Expected `dict[str, Any]`, found `dict[Unknown | bytes, Unknown | int]`"
type("Foo", (Base,), {b"attr": 1})
```

## `type[...]` as base class

`type[...]` (SubclassOf) types cannot be used as base classes. When a `type[...]` is used in the
bases tuple, we emit a diagnostic and insert `Unknown` into the MRO. This gives exactly one
diagnostic about the unsupported base, rather than cascading errors:

```py
from ty_extensions import reveal_mro

class Base:
    base_attr: int = 1

def f(x: type[Base]):
    # error: [unsupported-base] "Unsupported class base"
    Child = type("Child", (x,), {})

    # The class is still created with `Unknown` in MRO, allowing attribute access
    reveal_type(Child)  # revealed: <class 'Child'>
    reveal_mro(Child)  # revealed: (<class 'Child'>, Unknown, <class 'object'>)
    child = Child()
    reveal_type(child)  # revealed: Child

    # Attributes from `Unknown` are accessible without further errors
    reveal_type(child.base_attr)  # revealed: Unknown
```

## MRO errors

MRO errors are detected and reported:

```py
class A: ...

# Duplicate bases are detected
# error: [duplicate-base] "Duplicate base class <class 'A'> in class `Dup`"
Dup = type("Dup", (A, A), {})
```

Unknown bases (from unresolved imports) don't trigger duplicate-base diagnostics, since we can't
know if they represent the same type:

```py
from unresolved_module import Bar, Baz  # error: [unresolved-import]

# No duplicate-base error here - Bar and Baz are Unknown, and we can't
# know if they're the same type.
X = type("X", (Bar, Baz), {})
```

```py
class A: ...
class B(A): ...
class C(A): ...

# This creates an inconsistent MRO because D would need B before C (from first base)
# but also C before B (from second base inheritance through A)
class X(B, C): ...
class Y(C, B): ...

# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Conflict` with bases `[<class 'X'>, <class 'Y'>]`"
Conflict = type("Conflict", (X, Y), {})
```

## Metaclass conflicts

Metaclass conflicts are detected and reported:

```py
class Meta1(type): ...
class Meta2(type): ...
class A(metaclass=Meta1): ...
class B(metaclass=Meta2): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`Bad`) must be a subclass of the metaclasses of all its bases, but `Meta1` (metaclass of base class `<class 'A'>`) and `Meta2` (metaclass of base class `<class 'B'>`) have no subclass relationship"
Bad = type("Bad", (A, B), {})
```

## Cyclic dynamic class definitions

Self-referential class definitions using `type()` are detected. The name being defined is referenced
in the bases tuple before it's available:

```pyi
# error: [unresolved-reference] "Name `X` used when not defined"
X = type("X", (X,), {})
```

## Dynamic class names (non-literal strings)

When the class name is not a string literal, we still create a class literal type but with a
placeholder name `<unknown>`:

```py
def make_class(name: str):
    # When the name is a dynamic string, we use a placeholder name
    cls = type(name, (), {})
    reveal_type(cls)  # revealed: <class '<unknown>'>
    return cls

def make_classes(name1: str, name2: str):
    cls1 = type(name1, (), {})
    cls2 = type(name2, (), {})

    def inner(x: cls1): ...

    # error: [invalid-argument-type] "Argument to function `inner` is incorrect: Expected `mdtest_snippet.<locals of function 'make_classes'>.<unknown> @ src/mdtest_snippet.py:8`, found `mdtest_snippet.<locals of function 'make_classes'>.<unknown> @ src/mdtest_snippet.py:9`"
    inner(cls2())
```

When the name comes from a union of string literals, we also use a placeholder name:

```py
import random

name = "Foo" if random.random() > 0.5 else "Bar"
reveal_type(name)  # revealed: Literal["Foo", "Bar"]

# We cannot determine which name will be used at runtime
cls = type(name, (), {})
reveal_type(cls)  # revealed: <class '<unknown>'>
```

## Dynamic bases (variable tuple)

When the bases tuple is a function parameter with a non-literal tuple type, we still create a class
literal type but with `Unknown` in the MRO. This means instances are treated highly dynamically -
any attribute access returns `Unknown`:

```py
from ty_extensions import reveal_mro

class Base1: ...
class Base2: ...

def make_class(bases: tuple[type, ...]):
    # Class literal is created with Unknown base in MRO
    cls = type("Cls", bases, {})
    reveal_type(cls)  # revealed: <class 'Cls'>
    reveal_mro(cls)  # revealed: (<class 'Cls'>, Unknown, <class 'object'>)

    # Instances have dynamic attribute access due to Unknown base
    instance = cls()
    reveal_type(instance)  # revealed: Cls
    reveal_type(instance.any_attr)  # revealed: Unknown
    reveal_type(instance.any_method())  # revealed: Unknown

    return cls
```

When `bases` is a module-level variable holding a tuple of class literals, we can extract the base
classes:

```py
class Base:
    attr: int = 1

bases = (Base,)
Cls = type("Cls", bases, {})
reveal_type(Cls)  # revealed: <class 'Cls'>

instance = Cls()
reveal_type(instance.attr)  # revealed: int
```

## Variadic arguments

Unpacking arguments with `*args` or `**kwargs`:

```py
class Base: ...

# Unpacking a tuple for bases
bases_tuple = (Base,)
Cls1 = type("Cls1", (*bases_tuple,), {})
reveal_type(Cls1)  # revealed: <class 'Cls1'>

# Unpacking a dict for the namespace - the dict contents are not tracked anyway
namespace = {"attr": 1}
Cls2 = type("Cls2", (Base,), {**namespace})
reveal_type(Cls2)  # revealed: <class 'Cls2'>
```

When `*args` or `**kwargs` fill an unknown number of parameters, we cannot determine which overload
of `type()` is being called:

```py
def f(*args, **kwargs):
    # Completely dynamic: could be 1-arg or 3-arg form
    A = type(*args, **kwargs)
    reveal_type(A)  # revealed: type[Unknown]

    # Has a string first arg, but unknown additional args from *args
    B = type("B", *args, **kwargs)
    # TODO: `type[Unknown]` would cause fewer false positives
    reveal_type(B)  # revealed: <class 'str'>

    # Has string and tuple, but unknown additional args
    C = type("C", (), *args, **kwargs)
    # TODO: `type[Unknown]` would cause fewer false positives
    reveal_type(C)  # revealed: type

    # All three positional args provided, only **kwargs unknown
    D = type("D", (), {}, **kwargs)
    # TODO: `type[Unknown]` would cause fewer false positives
    reveal_type(D)  # revealed: type
```

## Explicit type annotations

When an explicit type annotation is provided, the inferred type is checked against it:

```py
# The annotation `type` is compatible with the inferred class literal type
T: type = type("T", (), {})
reveal_type(T)  # revealed: <class 'T'>

# The annotation `type[Base]` is compatible with the inferred type
class Base: ...

Derived: type[Base] = type("Derived", (Base,), {})
reveal_type(Derived)  # revealed: <class 'Derived'>

# Incompatible annotation produces an error
class Unrelated: ...

# error: [invalid-assignment]
Bad: type[Unrelated] = type("Bad", (Base,), {})
```

## Special base classes

Some special base classes work with dynamic class creation, but special semantics may not be fully
synthesized:

### Protocol bases

```py
# Protocol bases work - the class is created as a subclass of the protocol
from typing import Protocol

class MyProtocol(Protocol):
    def method(self) -> int: ...

ProtoImpl = type("ProtoImpl", (MyProtocol,), {})
reveal_type(ProtoImpl)  # revealed: <class 'ProtoImpl'>

instance = ProtoImpl()
reveal_type(instance)  # revealed: ProtoImpl
```

### TypedDict bases

```py
# TypedDict bases work but TypedDict semantics aren't applied to dynamic subclasses
from typing_extensions import TypedDict

class MyDict(TypedDict):
    name: str
    age: int

DictSubclass = type("DictSubclass", (MyDict,), {})
reveal_type(DictSubclass)  # revealed: <class 'DictSubclass'>
```

### NamedTuple bases

```py
# NamedTuple bases work but the dynamic subclass isn't recognized as a NamedTuple
from typing import NamedTuple

class Point(NamedTuple):
    x: int
    y: int

Point3D = type("Point3D", (Point,), {})
reveal_type(Point3D)  # revealed: <class 'Point3D'>
```

### Enum bases

```py
# Enum subclassing via type() is not supported - EnumMeta requires special dict handling
# that type() cannot provide. This applies even to empty enums.
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2

class EmptyEnum(Enum):
    pass

# TODO: We should emit a diagnostic here - type() cannot create Enum subclasses
ExtendedColor = type("ExtendedColor", (Color,), {})
reveal_type(ExtendedColor)  # revealed: <class 'ExtendedColor'>

# Even empty enums fail - EnumMeta requires special dict handling
# TODO: We should emit a diagnostic here too
ValidExtension = type("ValidExtension", (EmptyEnum,), {})
reveal_type(ValidExtension)  # revealed: <class 'ValidExtension'>
```

## `__init_subclass__` keyword arguments

When a base class defines `__init_subclass__` with required arguments, those should be passed to
`type()`. This is not yet supported:

```py
class Base:
    def __init_subclass__(cls, required_arg: str, **kwargs):
        super().__init_subclass__(**kwargs)
        cls.config = required_arg

# Regular class definition - this works and passes the argument
class Child(Base, required_arg="value"):
    pass

# The dynamically assigned attribute has Unknown in its type
reveal_type(Child.config)  # revealed: Unknown | str

# Dynamic class creation - keyword arguments are not yet supported
# TODO: This should work: type("DynamicChild", (Base,), {}, required_arg="value")
# error: [no-matching-overload]
DynamicChild = type("DynamicChild", (Base,), {}, required_arg="value")
```

## Empty bases tuple

When the bases tuple is empty, the class implicitly inherits from `object`:

```py
from ty_extensions import reveal_mro

EmptyBases = type("EmptyBases", (), {})
reveal_type(EmptyBases)  # revealed: <class 'EmptyBases'>
reveal_mro(EmptyBases)  # revealed: (<class 'EmptyBases'>, <class 'object'>)

instance = EmptyBases()
reveal_type(instance)  # revealed: EmptyBases

# object methods are available
reveal_type(instance.__hash__())  # revealed: int
reveal_type(instance.__str__())  # revealed: str
```

## Custom metaclass via bases

When a base class has a custom metaclass, the dynamic class inherits that metaclass:

```py
class MyMeta(type):
    custom_attr: str = "meta"

class Base(metaclass=MyMeta): ...

# Dynamic class inherits the metaclass from Base
Dynamic = type("Dynamic", (Base,), {})
reveal_type(Dynamic)  # revealed: <class 'Dynamic'>

# Metaclass attributes are accessible on the class
reveal_type(Dynamic.custom_attr)  # revealed: str
```
