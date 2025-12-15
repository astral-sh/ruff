# Implicit class body attributes

## Class body implicit attributes

Python makes certain names available implicitly inside class body scopes. These are `__qualname__`,
`__module__`, and `__doc__`, as documented at
<https://docs.python.org/3/reference/datamodel.html#creating-the-class-object>.

```py
class Foo:
    reveal_type(__qualname__)  # revealed: str
    reveal_type(__module__)  # revealed: str
    reveal_type(__doc__)  # revealed: str | None
```

## `__firstlineno__` (Python 3.13+)

Python 3.13 added `__firstlineno__` to the class body namespace.

### Available in Python 3.13+

```toml
[environment]
python-version = "3.13"
```

```py
class Foo:
    reveal_type(__firstlineno__)  # revealed: int
```

### Not available in Python 3.12 and earlier

```toml
[environment]
python-version = "3.12"
```

```py
class Foo:
    # error: [unresolved-reference]
    __firstlineno__
```

## Nested classes

These implicit attributes are also available in nested classes, and refer to the nested class:

```py
class Outer:
    class Inner:
        reveal_type(__qualname__)  # revealed: str
        reveal_type(__module__)  # revealed: str
```

## Class body implicit attributes have priority over globals

If a global variable with the same name exists, the class body implicit attribute takes priority
within the class body:

```py
__qualname__ = 42
__module__ = 42

class Foo:
    # Inside the class body, these are the implicit class attributes
    reveal_type(__qualname__)  # revealed: str
    reveal_type(__module__)  # revealed: str

# Outside the class, the globals are visible
reveal_type(__qualname__)  # revealed: Literal[42]
reveal_type(__module__)  # revealed: Literal[42]
```

## `__firstlineno__` has priority over globals (Python 3.13+)

The same applies to `__firstlineno__` on Python 3.13+:

```toml
[environment]
python-version = "3.13"
```

```py
__firstlineno__ = "not an int"

class Foo:
    reveal_type(__firstlineno__)  # revealed: int

reveal_type(__firstlineno__)  # revealed: Literal["not an int"]
```

## Class body implicit attributes are not visible in methods

The implicit class body attributes are only available directly in the class body, not in nested
function scopes (methods):

```py
class Foo:
    # Available directly in the class body
    x = __qualname__
    reveal_type(x)  # revealed: str

    def method(self):
        # Not available in methods - falls back to builtins/globals
        # error: [unresolved-reference]
        __qualname__
```

## Real-world use case: logging

A common use case is defining a logger with the class name:

```py
import logging

class MyClass:
    logger = logging.getLogger(__qualname__)
    reveal_type(logger)  # revealed: Logger
```
