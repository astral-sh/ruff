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

# error: [invalid-argument-type] "Invalid argument to parameter 2 (`bases`) of `type()`: Expected `tuple[type, ...]`, found `<class 'Base'>`"
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
