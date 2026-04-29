# Calls to `types.new_class()`

## Basic dynamic class creation

`types.new_class()` creates a new class dynamically. We infer a dynamic class type using the name
from the first argument and bases from the second argument.

```py
import types

class Base: ...
class Mixin: ...

# Basic call with no bases
reveal_type(types.new_class("Foo"))  # revealed: <class 'Foo'>

# With a single base class
reveal_type(types.new_class("Bar", (Base,)))  # revealed: <class 'Bar'>

# With multiple base classes
reveal_type(types.new_class("Baz", (Base, Mixin)))  # revealed: <class 'Baz'>
```

## Keyword arguments

Arguments can be passed as keyword arguments.

```py
import types

class Base: ...

reveal_type(types.new_class("Foo", bases=(Base,)))  # revealed: <class 'Foo'>
reveal_type(types.new_class(name="Bar"))  # revealed: <class 'Bar'>
reveal_type(types.new_class(name="Baz", bases=(Base,)))  # revealed: <class 'Baz'>
```

## Assignability to base type

The inferred type should be assignable to `type[Base]` when the class inherits from `Base`.

```py
import types

class Base: ...

tests: list[type[Base]] = []
NewFoo = types.new_class("NewFoo", (Base,))
tests.append(NewFoo)  # No error - type[NewFoo] is assignable to type[Base]
```

## Invalid calls

### Non-string name

```py
import types

class Base: ...

# error: [invalid-argument-type] "Invalid argument to parameter 1 (`name`) of `types.new_class()`: Expected `str`, found `Literal[123]`"
types.new_class(123, (Base,))
```

### Non-iterable bases

```py
import types

class Base: ...

# error: [invalid-argument-type] "Invalid argument to parameter 2 (`bases`) of `types.new_class()`: Expected `Iterable[object]`, found `<class 'Base'>`"
types.new_class("Foo", Base)
```

### Invalid base types

```py
import types

# error: [invalid-base] "Invalid class base with type `Literal[1]`"
# error: [invalid-base] "Invalid class base with type `Literal[2]`"
types.new_class("Foo", (1, 2))
```

### No arguments

```py
import types

# error: [no-matching-overload] "No overload of `types.new_class` matches arguments"
types.new_class()
```

### Invalid `kwds`

```py
import types

# error: [invalid-argument-type]
types.new_class("Foo", (), 1)
```

### Invalid `exec_body`

```py
import types

# error: [invalid-argument-type]
types.new_class("Foo", (), None, 1)
```

### Too many positional arguments

```py
import types

# error: [too-many-positional-arguments]
types.new_class("Foo", (), None, None, 1)
```

### Duplicate bases

```py
import types

class Base: ...

# error: [duplicate-base] "Duplicate base class <class 'Base'> in class `Dup`"
types.new_class("Dup", (Base, Base))
```

## Special bases

`types.new_class()` properly handles `__mro_entries__` and metaclasses, so it supports bases that
`type()` does not.

These cases are mostly about showing that class creation is valid and that ty preserves the base
information it can see. `types.new_class()` still doesn't let ty observe explicit class members
unless `exec_body` populates the namespace dynamically, and then attribute types become `Unknown`.

### Iterable bases

Any iterable of bases is accepted. When the iterable is a list literal, we should still preserve the
real base-class information:

```py
import types
from ty_extensions import reveal_mro

class Base:
    base_attr: int = 1

FromList = types.new_class("FromList", [Base])
reveal_type(FromList().base_attr)  # revealed: int

FromKeywordList = types.new_class("FromKeywordList", bases=[Base])
reveal_type(FromKeywordList().base_attr)  # revealed: int

bases = (Base,)
FromStarredList = types.new_class("FromStarredList", [*bases])
reveal_type(FromStarredList().base_attr)  # revealed: int
reveal_mro(FromStarredList)  # revealed: (<class 'FromStarredList'>, <class 'Base'>, <class 'object'>)
```

### Enum bases

Unlike `type()`, `types.new_class()` properly handles metaclasses, so inheriting from `enum.Enum` or
an empty enum subclass is valid:

```py
import types
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2

# Enums with members are still final and cannot be subclassed,
# regardless of whether we use type() or types.new_class()
# error: [subclass-of-final-class]
ExtendedColor = types.new_class("ExtendedColor", (Color,))

class EmptyEnum(Enum):
    pass

# Empty enum subclasses are fine with types.new_class() because it
# properly resolves and uses the EnumMeta metaclass
EmptyEnumSub = types.new_class("EmptyEnumSub", (EmptyEnum,))
reveal_type(EmptyEnumSub)  # revealed: <class 'EmptyEnumSub'>

# Directly inheriting from Enum is also fine
MyEnum = types.new_class("MyEnum", (Enum,))
reveal_type(MyEnum)  # revealed: <class 'MyEnum'>
```

### Explicit metaclass via `kwds`

Explicit metaclass overrides in `kwds` should affect the resulting class object the same way as a
`metaclass=` clause on a class statement:

```py
import types

class Meta(type):
    meta_attr: int = 1

ViaPositional = types.new_class("ViaPositional", (), {"metaclass": Meta})
reveal_type(ViaPositional.__class__)  # revealed: <class 'Meta'>
reveal_type(ViaPositional.meta_attr)  # revealed: int

ViaKeyword = types.new_class("ViaKeyword", (), kwds={"metaclass": Meta})
reveal_type(ViaKeyword.__class__)  # revealed: <class 'Meta'>
reveal_type(ViaKeyword.meta_attr)  # revealed: int

ViaKeywordDefaultBases = types.new_class("ViaKeywordDefaultBases", kwds={"metaclass": Meta})
reveal_type(ViaKeywordDefaultBases.__class__)  # revealed: <class 'Meta'>
reveal_type(ViaKeywordDefaultBases.meta_attr)  # revealed: int
```

### Only direct `kwds` dict literals are tracked

For soundness, ty only treats direct `kwds` dict literals as explicit metaclass overrides. Once the
dictionary flows through a variable, later mutations or missing keys can change the runtime
behavior:

```py
import types
from typing_extensions import NotRequired, TypedDict

class Meta(type):
    meta_attr: int = 1

kwds = {"metaclass": Meta}
ViaVariable = types.new_class("ViaVariable", (), kwds)
ViaVariable.meta_attr  # error: [unresolved-attribute]

def f():
    kwds = {"metaclass": Meta}
    del kwds["metaclass"]

    via_mutation = types.new_class("ViaMutation", (), kwds)
    via_mutation.meta_attr  # error: [unresolved-attribute]

class MaybeMetaKwds(TypedDict):
    metaclass: NotRequired[type[Meta]]

def g(kwds: MaybeMetaKwds):
    # error: [invalid-argument-type]
    via_typed_dict = types.new_class("ViaTypedDict", (), kwds=kwds)
    via_typed_dict.meta_attr  # error: [unresolved-attribute]
```

### Invalid explicit metaclass via `kwds`

Invalid explicit metaclass overrides should still report `invalid-metaclass`, whether the value is
definitely non-callable or only partly callable:

```py
import types
from typing import Any

# error: [invalid-metaclass] "Metaclass type `Literal[1]` is not callable"
types.new_class("NotCallable", (), {"metaclass": 1})

# error: [invalid-metaclass] "Metaclass type `Literal[1]` is not callable"
NotCallableDefaultBases = types.new_class("NotCallableDefaultBases", kwds={"metaclass": 1})

# error: [invalid-metaclass]
types.new_class("MaybeCallable", (), {"metaclass": Any})
```

### Invalid explicit metaclass with non-fixed iterable bases

Metaclass validity is independent of whether the `bases` iterable can be enumerated statically:

```py
import types
from collections.abc import Iterable

def f(bases: Iterable[object]):
    # error: [invalid-metaclass] "Metaclass type `Literal[1]` is not callable"
    types.new_class("NotCallableFromIterable", bases, {"metaclass": 1})
```

### Generic explicit metaclass via `kwds`

Unspecialized generic metaclasses are rejected the same way as on class statements:

```py
import types
from typing import Generic, TypeVar

T = TypeVar("T")

class GenericMeta(type, Generic[T]): ...

# error: [invalid-metaclass] "Generic metaclasses are not supported"
types.new_class("GenericMetaInstance", (), {"metaclass": GenericMeta[T]})
```

### Explicit metaclass conflicts

An explicit metaclass override still has to be compatible with the metaclasses of all bases:

```py
import types

class M1(type): ...
class M2(type): ...
class Base(metaclass=M1): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`Derived`) must be a subclass of the metaclasses of all its bases, but `M2` (metaclass of `Derived`) and `M1` (metaclass of base class `Base`) have no subclass relationship"
Derived = types.new_class("Derived", (Base,), {"metaclass": M2})
reveal_type(Derived.__class__)  # revealed: type[Unknown]

meta: type[M2] = M2

# error: [conflicting-metaclass] "The metaclass of a derived class (`DerivedViaVariable`) must be a subclass of the metaclasses of all its bases, but `M2` (metaclass of `DerivedViaVariable`) and `M1` (metaclass of base class `Base`) have no subclass relationship"
DerivedViaVariable = types.new_class("DerivedViaVariable", (Base,), {"metaclass": meta})
reveal_type(DerivedViaVariable.__class__)  # revealed: type[Unknown]
```

### Generic and TypedDict bases

Even though `types.new_class()` handles `__mro_entries__` at runtime, ty does not yet model the full
typing semantics of dynamically-created generic classes or TypedDicts, so these bases are rejected:

```py
import types
from typing import Generic, TypeVar
from typing_extensions import TypedDict

T = TypeVar("T")

# error: [invalid-base] "Invalid base for class created via `types.new_class()`"
GenericClass = types.new_class("GenericClass", (Generic[T],))

# error: [invalid-base] "Invalid base for class created via `types.new_class()`"
TypedDictClass = types.new_class("TypedDictClass", (TypedDict,))
```

### `type[X]` bases

`type[X]` represents "some subclass of X". This is a valid base class, but the exact class is not
known, so the MRO cannot be resolved. `Unknown` is inserted and `unsupported-dynamic-base` is
emitted:

```py
import types
from ty_extensions import reveal_mro

class Base:
    base_attr: int = 1

def f(x: type[Base]):
    # error: [unsupported-dynamic-base] "Unsupported class base"
    Child = types.new_class("Child", (x,))

    reveal_type(Child)  # revealed: <class 'Child'>
    reveal_mro(Child)  # revealed: (<class 'Child'>, Unknown, <class 'object'>)
    child = Child()
    reveal_type(child.base_attr)  # revealed: Unknown
```

`type[Any]` and `type[Unknown]` already carry the dynamic kind, so no diagnostic is needed. An
unknowable MRO is already inherent to `Any`/`Unknown`:

```py
import types
from typing import Any

def g(x: type[Any]):
    # No diagnostic: `Any` base is fine as-is
    Child = types.new_class("Child", (x,))
    reveal_type(Child)  # revealed: <class 'Child'>
```

## Dynamic namespace via `exec_body`

When `exec_body` is provided, it can populate the class namespace dynamically, so attribute access
returns `Unknown`. Without `exec_body`, the namespace is empty and attribute access is an error:

```py
import types

class Base:
    base_attr: int = 1

# Without exec_body: no dynamic namespace, so only base attributes are available
NoBody = types.new_class("NoBody", (Base,))
instance = NoBody()
reveal_type(instance.base_attr)  # revealed: int
instance.missing_attr  # error: [unresolved-attribute]

# With exec_body=None: same as no exec_body
NoBodyExplicit = types.new_class("NoBodyExplicit", (Base,), exec_body=None)
instance_explicit = NoBodyExplicit()
reveal_type(instance_explicit.base_attr)  # revealed: int
instance_explicit.missing_attr  # error: [unresolved-attribute]

# With exec_body=None passed positionally: same as no exec_body
NoBodyPositional = types.new_class("NoBodyPositional", (Base,), None, None)
instance_positional = NoBodyPositional()
reveal_type(instance_positional.base_attr)  # revealed: int
instance_positional.missing_attr  # error: [unresolved-attribute]

# With exec_body statically known to be None: same as no exec_body
body_none: None = None
NoBodyInVariable = types.new_class("NoBodyInVariable", (Base,), exec_body=body_none)
instance_variable = NoBodyInVariable()
reveal_type(instance_variable.base_attr)  # revealed: int
instance_variable.missing_attr  # error: [unresolved-attribute]

# With exec_body: namespace is dynamic, so any attribute access returns Unknown
def body(ns):
    ns["x"] = 1

WithBody = types.new_class("WithBody", (Base,), exec_body=body)
instance2 = WithBody()
reveal_type(instance2.x)  # revealed: Unknown
reveal_type(instance2.base_attr)  # revealed: Unknown
```

## Later dynamic `kwds` entries can override `metaclass`

If a later dict entry could overwrite `metaclass`, ty should stop treating the explicit metaclass as
guaranteed. Earlier dynamic entries are fine because a later literal `metaclass` still wins:

```py
import types

class Meta(type):
    meta_attr: int = 1

def f(other: dict[str, object], key: str, value: object):
    via_unpack = types.new_class("ViaUnpack", (), {"metaclass": Meta, **other})
    via_unpack.meta_attr  # error: [unresolved-attribute]

    via_dynamic_key = types.new_class("ViaDynamicKey", (), {"metaclass": Meta, key: value})
    via_dynamic_key.meta_attr  # error: [unresolved-attribute]

    via_last_write = types.new_class("ViaLastWrite", (), {**other, "metaclass": Meta})
    reveal_type(via_last_write.meta_attr)  # revealed: int
```

## `kwds` branches without a guaranteed metaclass

If some control-flow paths don't provide a `metaclass` entry, the resulting class should not be
treated as if the explicit metaclass were guaranteed:

```py
import types

class Meta(type):
    meta_attr: int = 1

def f(flag: bool):
    kwds = None
    if flag:
        kwds = {"metaclass": Meta}

    maybe_meta = types.new_class("MaybeMeta", (), kwds)
    maybe_meta.meta_attr  # error: [unresolved-attribute]
```

## Forward references via string annotations

Forward references via subscript annotations on generic bases are supported:

```py
import types

# Forward reference to X via subscript annotation in tuple base
# (This fails at runtime, but we should handle it without panicking)
X = types.new_class("X", (tuple["X | None"],))
reveal_type(X)  # revealed: <class 'X'>
```
