# Calling builtins

## `bool` with incorrect arguments

```py
class NotBool:
    __bool__ = None

# error: [too-many-positional-arguments] "Too many positional arguments to class `bool`: expected 1, got 2"
bool(1, 2)

# TODO: We should emit an `unsupported-bool-conversion` error here because the argument doesn't implement `__bool__` correctly.
bool(NotBool())
```

## Calls to `type()`

A single-argument call to `type()` returns an object that has the argument's meta-type. (This is
tested more extensively in `crates/ty_python_semantic/resources/mdtest/attributes.md`, alongside the
tests for the `__class__` attribute.)

```py
reveal_type(type(1))  # revealed: <class 'int'>
```

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

Attributes from the namespace dict (third argument) are not tracked. Like Pyright, we error when
attempting to access them:

```py
class Base: ...

Foo = type("Foo", (Base,), {"custom_attr": 42})
foo = Foo()

# error: [unresolved-attribute] "Object of type `Foo` has no attribute `custom_attr`"
reveal_type(foo.custom_attr)  # revealed: Unknown
```

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
```

Functional classes are correctly recognized as disjoint from unrelated types:

```py
class Base: ...

Foo = type("Foo", (Base,), {})

def check_disjointness(x: Foo | int) -> None:
    if isinstance(x, int):
        reveal_type(x)  # revealed: int
    else:
        # Foo and int are not considered disjoint because `class C(Foo, int)` could exist.
        reveal_type(x)  # revealed: Foo & ~int

# Functional class inheriting from int is NOT disjoint from int
IntSubclass = type("IntSubclass", (int,), {})

def check_int_subclass(x: IntSubclass | str) -> None:
    if isinstance(x, int):
        # IntSubclass inherits from int, so it's included in the narrowed type
        reveal_type(x)  # revealed: IntSubclass
    else:
        reveal_type(x)  # revealed: str
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

Functional classes can be used as pivot in `super()`:

```py
class Base:
    def method(self) -> int:
        return 42

FunctionalChild = type("FunctionalChild", (Base,), {})

# Using functional class as pivot with functional class instance owner
fc = FunctionalChild()
reveal_type(super(FunctionalChild, fc))  # revealed: <super: FunctionalChild, FunctionalChild>
reveal_type(super(FunctionalChild, fc).method())  # revealed: int

# Regular class inheriting from functional class
class RegularChild(FunctionalChild):
    pass

rc = RegularChild()
reveal_type(super(RegularChild, rc))  # revealed: <super: <class 'RegularChild'>, RegularChild>
reveal_type(super(RegularChild, rc).method())  # revealed: int

# Using functional class as pivot with regular class instance owner
reveal_type(super(FunctionalChild, rc))  # revealed: <super: FunctionalChild, RegularChild>
reveal_type(super(FunctionalChild, rc).method())  # revealed: int
```

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

Other numbers of arguments are invalid

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

Metaclass conflicts are detected and reported:

```py
class Meta1(type): ...
class Meta2(type): ...
class A(metaclass=Meta1): ...
class B(metaclass=Meta2): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`Bad`) must be a subclass of the metaclasses of all its bases, but `Meta1` (metaclass of base class `<class 'A'>`) and `Meta2` (metaclass of base class `<class 'B'>`) have no subclass relationship"
Bad = type("Bad", (A, B), {})
```

## Calls to `str()`

### Valid calls

```py
str()
str("")
str(b"")
str(1)
str(object=1)

str(b"M\xc3\xbcsli", "utf-8")
str(b"M\xc3\xbcsli", "utf-8", "replace")

str(b"M\x00\xfc\x00s\x00l\x00i\x00", encoding="utf-16")
str(b"M\x00\xfc\x00s\x00l\x00i\x00", encoding="utf-16", errors="ignore")

str(bytearray.fromhex("4d c3 bc 73 6c 69"), "utf-8")
str(bytearray(), "utf-8")

str(encoding="utf-8", object=b"M\xc3\xbcsli")
str(b"", errors="replace")
str(encoding="utf-8")
str(errors="replace")
```

### Invalid calls

```py
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `bytes | bytearray`, found `Literal[1]`"
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `str`, found `Literal[2]`"
str(1, 2)

# error: [no-matching-overload]
str(o=1)

# First argument is not a bytes-like object:
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `bytes | bytearray`, found `Literal["Müsli"]`"
str("Müsli", "utf-8")

# Second argument is not a valid encoding:
# error: [invalid-argument-type] "Argument to class `str` is incorrect: Expected `str`, found `Literal[b"utf-8"]`"
str(b"M\xc3\xbcsli", b"utf-8")
```

## Calls to `isinstance`

We infer `Literal[True]` for a limited set of cases where we can be sure that the answer is correct,
but fall back to `bool` otherwise.

```py
from enum import Enum
from types import FunctionType
from typing import TypeVar

class Answer(Enum):
    NO = 0
    YES = 1

reveal_type(isinstance(True, bool))  # revealed: Literal[True]
reveal_type(isinstance(True, int))  # revealed: Literal[True]
reveal_type(isinstance(True, object))  # revealed: Literal[True]
reveal_type(isinstance("", str))  # revealed: Literal[True]
reveal_type(isinstance(1, int))  # revealed: Literal[True]
reveal_type(isinstance(b"", bytes))  # revealed: Literal[True]
reveal_type(isinstance(Answer.NO, Answer))  # revealed: Literal[True]

reveal_type(isinstance((1, 2), tuple))  # revealed: Literal[True]

def f(): ...

reveal_type(isinstance(f, FunctionType))  # revealed: Literal[True]

reveal_type(isinstance("", int))  # revealed: bool

class A: ...
class SubclassOfA(A): ...
class OtherSubclassOfA(A): ...
class B: ...

reveal_type(isinstance(A, type))  # revealed: Literal[True]

a = A()

reveal_type(isinstance(a, A))  # revealed: Literal[True]
reveal_type(isinstance(a, object))  # revealed: Literal[True]
reveal_type(isinstance(a, SubclassOfA))  # revealed: bool
reveal_type(isinstance(a, B))  # revealed: bool

s = SubclassOfA()
reveal_type(isinstance(s, SubclassOfA))  # revealed: Literal[True]
reveal_type(isinstance(s, A))  # revealed: Literal[True]

def _(x: A | B, y: list[int]):
    reveal_type(isinstance(y, list))  # revealed: Literal[True]
    reveal_type(isinstance(x, A))  # revealed: bool

    if isinstance(x, A):
        pass
    else:
        reveal_type(x)  # revealed: B & ~A
        reveal_type(isinstance(x, B))  # revealed: Literal[True]

T = TypeVar("T")
T_bound_A = TypeVar("T_bound_A", bound=A)
T_constrained = TypeVar("T_constrained", SubclassOfA, OtherSubclassOfA)

def _(
    x: T,
    x_bound_a: T_bound_A,
    x_constrained_sub_a: T_constrained,
):
    reveal_type(isinstance(x, object))  # revealed: Literal[True]
    reveal_type(isinstance(x, A))  # revealed: bool

    reveal_type(isinstance(x_bound_a, object))  # revealed: Literal[True]
    reveal_type(isinstance(x_bound_a, A))  # revealed: Literal[True]
    reveal_type(isinstance(x_bound_a, SubclassOfA))  # revealed: bool
    reveal_type(isinstance(x_bound_a, B))  # revealed: bool

    reveal_type(isinstance(x_constrained_sub_a, object))  # revealed: Literal[True]
    reveal_type(isinstance(x_constrained_sub_a, A))  # revealed: Literal[True]
    reveal_type(isinstance(x_constrained_sub_a, SubclassOfA))  # revealed: bool
    reveal_type(isinstance(x_constrained_sub_a, OtherSubclassOfA))  # revealed: bool
    reveal_type(isinstance(x_constrained_sub_a, B))  # revealed: bool
```

Certain special forms in the typing module are not instances of `type`, so are strictly-speaking
disallowed as the second argument to `isinstance()` according to typeshed's annotations. However, at
runtime they work fine as the second argument, and we implement that special case in ty:

```py
import typing as t

# no errors emitted for any of these:
isinstance("", t.Dict)
isinstance("", t.List)
isinstance("", t.Set)
isinstance("", t.FrozenSet)
isinstance("", t.Tuple)
isinstance("", t.ChainMap)
isinstance("", t.Counter)
isinstance("", t.Deque)
isinstance("", t.OrderedDict)
isinstance("", t.Callable)
isinstance("", t.Type)
isinstance("", t.Callable | t.Deque)

# `Any` is valid in `issubclass()` calls but not `isinstance()` calls
issubclass(list, t.Any)
issubclass(list, t.Any | t.Dict)
```

But for other special forms that are not permitted as the second argument, we still emit an error:

```py
isinstance("", t.TypeGuard)  # error: [invalid-argument-type]
isinstance("", t.ClassVar)  # error: [invalid-argument-type]
isinstance("", t.Final)  # error: [invalid-argument-type]
isinstance("", t.Any)  # error: [invalid-argument-type]
```

## The builtin `NotImplemented` constant is not callable

<!-- snapshot-diagnostics -->

```py
raise NotImplemented()  # error: [call-non-callable]
raise NotImplemented("this module is not implemented yet!!!")  # error: [call-non-callable]
```
