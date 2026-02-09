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

## Keyword argument for bases

The `bases` argument can also be passed as a keyword argument.

```py
import types

class Base: ...

reveal_type(types.new_class("Foo", bases=(Base,)))  # revealed: <class 'Foo'>
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
