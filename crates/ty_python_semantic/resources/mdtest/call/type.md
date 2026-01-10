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

Instances of functional classes are typed with the synthesized class name. Attributes from all base
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

Attributes from the namespace dict (third argument) are tracked:

```py
class Base: ...

Foo = type("Foo", (Base,), {"custom_attr": 42})

# Class attribute access
reveal_type(Foo.custom_attr)  # revealed: Literal[42]

# Instance attribute access
foo = Foo()
reveal_type(foo.custom_attr)  # revealed: Literal[42]
```

When the namespace dict is not a literal (e.g., passed as a parameter), attribute access returns
`Unknown` since we can't know what attributes might be defined:

```py
from typing import Any

class DynamicBase: ...

def f(attributes: dict[str, Any]):
    X = type("X", (DynamicBase,), attributes)

    reveal_type(X)  # revealed: <class 'X'>

    # Attribute access returns Unknown since the namespace is dynamic
    reveal_type(X.foo)  # revealed: Unknown

    x = X()
    reveal_type(x.bar)  # revealed: Unknown
```

When a `TypedDict` is passed as the namespace argument, we synthesize a class type with the known
keys from the `TypedDict` as attributes. Since `TypedDict` instances are "open" (they can have
arbitrary additional string keys), unknown attributes return `Unknown`:

```py
from typing import TypedDict

class Namespace(TypedDict):
    z: int

def g(attributes: Namespace):
    Y = type("Y", (), attributes)

    reveal_type(Y)  # revealed: <class 'Y'>

    # Known keys from the TypedDict are tracked as attributes
    reveal_type(Y.z)  # revealed: int

    y = Y()
    reveal_type(y.z)  # revealed: int

    # Unknown attributes return Unknown since TypedDicts are open
    reveal_type(Y.unknown)  # revealed: Unknown
    reveal_type(y.unknown)  # revealed: Unknown
```

## Inheritance from functional classes

Regular classes can inherit from functional classes:

```py
class Base:
    base_attr: int = 1

FunctionalClass = type("FunctionalClass", (Base,), {})

class Child(FunctionalClass):
    child_attr: str = "child"

child = Child()

# Attributes from the functional class's base are accessible
reveal_type(child.base_attr)  # revealed: int

# The child class's own attributes are accessible
reveal_type(child.child_attr)  # revealed: str

# Child instances are subtypes of FunctionalClass instances
def takes_functional(x: FunctionalClass) -> None: ...

takes_functional(child)  # No error - Child is a subtype of FunctionalClass

# isinstance narrows to the functional class instance type
def check_isinstance(x: object) -> None:
    if isinstance(x, FunctionalClass):
        reveal_type(x)  # revealed: FunctionalClass

# Functional class inheriting from int narrows correctly with isinstance
IntSubclass = type("IntSubclass", (int,), {})

def check_int_subclass(x: IntSubclass | str) -> None:
    if isinstance(x, int):
        # IntSubclass inherits from int, so it's included in the narrowed type
        reveal_type(x)  # revealed: IntSubclass
    else:
        reveal_type(x)  # revealed: str
```

## Disjointness

Functional classes are not considered disjoint from unrelated types (since a subclass could inherit
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

Disjointness also works for `type[]` of functional classes:

```py
from ty_extensions import is_disjoint_from, static_assert

# Functional classes with disjoint bases have disjoint type[] types.
IntClass = type("IntClass", (int,), {})
StrClass = type("StrClass", (str,), {})

static_assert(is_disjoint_from(type[IntClass], type[StrClass]))
static_assert(is_disjoint_from(type[StrClass], type[IntClass]))

# Functional classes that share a common base are not disjoint.
class Base: ...

Foo = type("Foo", (Base,), {})
Bar = type("Bar", (Base,), {})

static_assert(not is_disjoint_from(type[Foo], type[Bar]))
```

## Using functional classes with `super()`

Functional classes can be used as pivot in `super()`:

```py
class Base:
    def method(self) -> int:
        return 42

FunctionalChild = type("FunctionalChild", (Base,), {})

# Using functional class as pivot with functional class instance owner
fc = FunctionalChild()
reveal_type(super(FunctionalChild, fc))  # revealed: <super: <class 'FunctionalChild'>, FunctionalChild>
reveal_type(super(FunctionalChild, fc).method())  # revealed: int

# Regular class inheriting from functional class
class RegularChild(FunctionalChild):
    pass

rc = RegularChild()
reveal_type(super(RegularChild, rc))  # revealed: <super: <class 'RegularChild'>, RegularChild>
reveal_type(super(RegularChild, rc).method())  # revealed: int

# Using functional class as pivot with regular class instance owner
reveal_type(super(FunctionalChild, rc))  # revealed: <super: <class 'FunctionalChild'>, RegularChild>
reveal_type(super(FunctionalChild, rc).method())  # revealed: int
```

## Functional class inheritance chains

Functional classes can inherit from other functional classes:

```py
class Base:
    base_attr: int = 1

# Create a functional class that inherits from a regular class.
Parent = type("Parent", (Base,), {})
reveal_type(Parent)  # revealed: <class 'Parent'>

# Create a functional class that inherits from another functional class.
ChildCls = type("ChildCls", (Parent,), {})
reveal_type(ChildCls)  # revealed: <class 'ChildCls'>

# Child instances have access to attributes from the entire inheritance chain.
child = ChildCls()
reveal_type(child)  # revealed: ChildCls
reveal_type(child.base_attr)  # revealed: int

# Child instances are subtypes of Parent instances.
def takes_parent(x: Parent) -> None: ...

takes_parent(child)  # No error - ChildCls is a subtype of Parent
```

## Dataclass transform inheritance

Functional classes that inherit from a `@dataclass_transform()` decorated base class are recognized
as dataclass-like and have the synthesized `__dataclass_fields__` attribute:

```py
from dataclasses import Field
from typing_extensions import dataclass_transform

@dataclass_transform()
class DataclassBase:
    """Base class decorated with @dataclass_transform()."""

    pass

# A functional class inheriting from a dataclass_transform base
DynamicModel = type("DynamicModel", (DataclassBase,), {})

# The functional class has __dataclass_fields__ synthesized
reveal_type(DynamicModel.__dataclass_fields__)  # revealed: dict[str, Field[Any]]
```

## Applying `@dataclass` decorator directly

Applying the `@dataclass` decorator directly to a functional class is not yet supported. This would
require tracking decorator applications to dynamic classes:

```py
from dataclasses import dataclass

# TODO: This should work but currently doesn't recognize Foo as a dataclass
Foo = type("Foo", (), {})
Foo = dataclass(Foo)

# Currently resolves to `Unknown` because the decorator's return type loses the specific class type.
# The `@dataclass` decorator has a complex overloaded signature and we don't track that
# it returns the same class it was given.
reveal_type(Foo.__dataclass_fields__)  # revealed: Unknown
```

## Generic base classes

Functional classes with generic base classes:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Container(Generic[T]):
    value: T

# Functional class inheriting from a generic class specialization
IntContainer = type("IntContainer", (Container[int],), {})
reveal_type(IntContainer)  # revealed: <class 'IntContainer'>

container = IntContainer()
reveal_type(container)  # revealed: IntContainer
reveal_type(container.value)  # revealed: int
```

## `type()` and `__class__` on functional instances

`type(instance)` returns the class of the functional instance:

```py
class Base: ...

Foo = type("Foo", (Base,), {})
foo = Foo()

# type() on an instance returns the class
reveal_type(type(foo))  # revealed: type[Foo]
```

`__class__` attribute access on functional instances:

```py
class Base: ...

Foo = type("Foo", (Base,), {})
foo = Foo()

# __class__ returns the class type
reveal_type(foo.__class__)  # revealed: type[Foo]
```

`__class__` on the functional class itself returns the metaclass (consistent with static classes):

```py
class StaticClass: ...

DynamicClass = type("DynamicClass", (), {})

# Both static and dynamic classes have `type` as their metaclass
reveal_type(StaticClass.__class__)  # revealed: <class 'type'>
reveal_type(DynamicClass.__class__)  # revealed: <class 'type'>
```

## Subtype relationships

Functional instances are subtypes of `object`:

```py
class Base: ...

Foo = type("Foo", (Base,), {})
foo = Foo()

# All functional instances are subtypes of object
def takes_object(x: object) -> None: ...

takes_object(foo)  # No error - Foo is a subtype of object

# Even functional classes with no explicit bases are subtypes of object
EmptyBases = type("EmptyBases", (), {})
empty = EmptyBases()
takes_object(empty)  # No error
```

## Attributes from `builtins.type`

Attributes defined on `builtins.type` are accessible on dynamic classes:

```py
T = type("T", (), {})

# Inherited from `builtins.type`:
# TODO: these should work but currently don't resolve
# reveal_type(T.__dictoffset__)  # revealed: int
# reveal_type(T.__name__)  # revealed: str
# reveal_type(T.__bases__)  # revealed: tuple[type, ...]
# reveal_type(T.__mro__)  # revealed: tuple[type, ...]
```

## Invalid calls

Other numbers of arguments are invalid:

```py
# error: [no-matching-overload] "No overload of class `type` matches arguments"
type("Foo", ())

# error: [no-matching-overload] "No overload of class `type` matches arguments"
type("Foo", (), {}, weird_other_arg=42)
```

The following calls are also invalid, due to incorrect argument types:

```py
class Base: ...

# error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `str`, found `Literal[b"Foo"]`"
type(b"Foo", (), {})

# error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `tuple[type, ...]`, found `<class 'Base'>`"
type("Foo", Base, {})

# error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `tuple[type, ...]`, found `tuple[Literal[1], Literal[2]]`"
type("Foo", (1, 2), {})

# error: [invalid-argument-type] "Argument to class `type` is incorrect: Expected `dict[str, Any]`, found `dict[str | bytes, Any]`"
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

## `__slots__` in namespace dictionary

Functional classes can define `__slots__` in the namespace dictionary. Non-empty `__slots__` makes
the class a "disjoint base", which prevents it from being used alongside other disjoint bases in a
class hierarchy:

```py
# Functional class with non-empty __slots__
Slotted = type("Slotted", (), {"__slots__": ("x", "y")})
slotted = Slotted()
reveal_type(slotted)  # revealed: Slotted

# Classes with empty __slots__ are not disjoint bases
EmptySlots = type("EmptySlots", (), {"__slots__": ()})

# Classes with no __slots__ are not disjoint bases
NoSlots = type("NoSlots", (), {})

# String __slots__ are treated as a single slot (non-empty)
StringSlots = type("StringSlots", (), {"__slots__": "x"})
```

Functional classes with non-empty `__slots__` cannot coexist with other disjoint bases:

```py
class RegularSlotted:
    __slots__ = ("a",)

# error: [instance-layout-conflict]
class Conflict(
    RegularSlotted,
    type("FuncSlotted", (), {"__slots__": ("b",)}),
): ...
```

Two functional classes with non-empty `__slots__` also conflict:

```py
A = type("A", (), {"__slots__": ("x",)})
B = type("B", (), {"__slots__": ("y",)})

# error: [instance-layout-conflict]
class Conflict(
    A,
    B,
): ...
```

When the namespace dictionary is dynamic (not a literal), we can't determine if `__slots__` is
defined, so no diagnostic is emitted:

```py
from typing import Any

class SlottedBase:
    __slots__ = ("a",)

def f(ns: dict[str, Any]):
    # The namespace might or might not contain __slots__, so no error is emitted
    Dynamic = type("Dynamic", (), ns)

    # No error: we can't prove there's a conflict since ns might not have __slots__
    class MaybeConflict(SlottedBase, Dynamic): ...
```

## Cyclic functional class definitions

Self-referential class definitions using `type()` are detected. The name being defined is referenced
in the bases tuple before it's available:

```pyi
# error: [unresolved-reference] "Name `X` used when not defined"
X = type("X", (X,), {})
```
