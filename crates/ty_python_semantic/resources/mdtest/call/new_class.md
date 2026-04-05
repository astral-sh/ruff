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

class Base:
    base_attr: int = 1

FromList = types.new_class("FromList", [Base])
reveal_type(FromList().base_attr)  # revealed: int

FromKeywordList = types.new_class("FromKeywordList", bases=[Base])
reveal_type(FromKeywordList().base_attr)  # revealed: int

bases = (Base,)
FromStarredList = types.new_class("FromStarredList", [*bases])
reveal_type(FromStarredList().base_attr)  # revealed: int
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

# With exec_body: namespace is dynamic, so any attribute access returns Unknown
def body(ns):
    ns["x"] = 1

WithBody = types.new_class("WithBody", (Base,), exec_body=body)
instance2 = WithBody()
reveal_type(instance2.x)  # revealed: Unknown
reveal_type(instance2.base_attr)  # revealed: Unknown
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
