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

## Assignment definition

When assigned to a variable, the dynamic class is identified by the definition.

```py
import types

class Base: ...

MyClass = types.new_class("MyClass", (Base,))
reveal_type(MyClass)  # revealed: <class 'MyClass'>

instance = MyClass()
reveal_type(instance)  # revealed: MyClass
```

## Invalid calls

### Non-string name

```py
import types

class Base: ...

# error: [invalid-argument-type] "Invalid argument to parameter 1 (`name`) of `types.new_class()`: Expected `str`, found `Literal[123]`"
types.new_class(123, (Base,))
```

### Non-tuple bases

```py
import types

class Base: ...

# error: [invalid-argument-type] "Invalid argument to parameter 2 (`bases`) of `types.new_class()`: Expected `tuple[type, ...]`, found `<class 'Base'>`"
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

`type()` doesn't support `__mro_entries__`, so `Generic[T]` and `TypedDict` fail as bases for
`type()`. `types.new_class()` handles `__mro_entries__` properly, so these are valid:

```py
import types
from typing import Generic, TypeVar
from typing_extensions import TypedDict

T = TypeVar("T")

GenericClass = types.new_class("GenericClass", (Generic[T],))
reveal_type(GenericClass)  # revealed: <class 'GenericClass'>

TypedDictClass = types.new_class("TypedDictClass", (TypedDict,))
reveal_type(TypedDictClass)  # revealed: <class 'TypedDictClass'>
```

### `type[X]` bases

`type[X]` represents "some subclass of X". This is a valid base class, but ty cannot determine the
exact class, so it cannot solve the MRO. `Unknown` is inserted and `unsupported-dynamic-base` is
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

`type[Any]` and `type[Unknown]` already carry the dynamic kind, so no diagnostic is needed — the MRO
being unknowable is inherent to `Any`/`Unknown`, not a ty limitation:

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

# With exec_body=None: same as no exec_body
NoBodyExplicit = types.new_class("NoBodyExplicit", (Base,), exec_body=None)
instance_explicit = NoBodyExplicit()
reveal_type(instance_explicit.base_attr)  # revealed: int

# With exec_body=None passed positionally: same as no exec_body
NoBodyPositional = types.new_class("NoBodyPositional", (Base,), None, None)
instance_positional = NoBodyPositional()
reveal_type(instance_positional.base_attr)  # revealed: int

# With exec_body: namespace is dynamic, so any attribute access returns Unknown
def body(ns):
    ns["x"] = 1

WithBody = types.new_class("WithBody", (Base,), exec_body=body)
instance2 = WithBody()
reveal_type(instance2.x)  # revealed: Unknown
reveal_type(instance2.anything)  # revealed: Unknown
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
