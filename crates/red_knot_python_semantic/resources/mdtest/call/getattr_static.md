# `inspect.getattr_static`

## Basic usage

`inspect.getattr_static` is a function that returns attributes of an object without invoking the
descriptor protocol (for caveats, see the [official documentation]).

Consider the following example:

```py
import inspect

class Descriptor:
    def __get__(self, instance, owner) -> str:
        return "a"

class C:
    normal: int = 1
    descriptor: Descriptor = Descriptor()
```

If we access attributes on an instance of `C` as usual, the descriptor protocol is invoked, and we
get a type of `str` for the `descriptor` attribute:

```py
c = C()

reveal_type(c.normal)  # revealed: int
reveal_type(c.descriptor)  # revealed: str
```

However, if we use `inspect.getattr_static`, we can see the underlying `Descriptor` type:

```py
reveal_type(inspect.getattr_static(c, "normal"))  # revealed: int
reveal_type(inspect.getattr_static(c, "descriptor"))  # revealed: Descriptor
```

For non-existent attributes, a default value can be provided:

```py
reveal_type(inspect.getattr_static(C, "normal", "default-arg"))  # revealed: int
reveal_type(inspect.getattr_static(C, "non_existent", "default-arg"))  # revealed: Literal["default-arg"]
```

When a non-existent attribute is accessed without a default value, the runtime raises an
`AttributeError`. We could emit a diagnostic for this case, but that is currently not supported:

```py
# TODO: we could emit a diagnostic here
reveal_type(inspect.getattr_static(C, "non_existent"))  # revealed: Never
```

We can access attributes on objects of all kinds:

```py
import sys

reveal_type(inspect.getattr_static(sys, "platform"))  # revealed: LiteralString
reveal_type(inspect.getattr_static(inspect, "getattr_static"))  # revealed: Literal[getattr_static]

reveal_type(inspect.getattr_static(1, "real"))  # revealed: property
```

(Implicit) instance attributes can also be accessed through `inspect.getattr_static`:

```py
class D:
    def __init__(self) -> None:
        self.instance_attr: int = 1

reveal_type(inspect.getattr_static(D(), "instance_attr"))  # revealed: int
```

And attributes on metaclasses can be accessed when probing the class:

```py
class Meta(type):
    attr: int = 1

class E(metaclass=Meta): ...

reveal_type(inspect.getattr_static(E, "attr"))  # revealed: int
```

Metaclass attributes can not be added when probing an instance of the class:

```py
reveal_type(inspect.getattr_static(E(), "attr", "non_existent"))  # revealed: Literal["non_existent"]
```

## Error cases

We can only infer precise types if the attribute is a literal string. In all other cases, we fall
back to `Any`:

```py
import inspect

class C:
    x: int = 1

def _(attr_name: str):
    reveal_type(inspect.getattr_static(C(), attr_name))  # revealed: Any
    reveal_type(inspect.getattr_static(C(), attr_name, 1))  # revealed: Any
```

But we still detect errors in the number or type of arguments:

```py
# error: [missing-argument] "No arguments provided for required parameters `obj`, `attr` of function `getattr_static`"
inspect.getattr_static()

# error: [missing-argument] "No argument provided for required parameter `attr`"
inspect.getattr_static(C())

# error: [invalid-argument-type] "Object of type `Literal[1]` cannot be assigned to parameter 2 (`attr`) of function `getattr_static`; expected type `str`"
inspect.getattr_static(C(), 1)

# error: [too-many-positional-arguments] "Too many positional arguments to function `getattr_static`: expected 3, got 4"
inspect.getattr_static(C(), "x", "default-arg", "one too many")
```

## Possibly unbound attributes

```py
import inspect

def _(flag: bool):
    class C:
        if flag:
            x: int = 1

    reveal_type(inspect.getattr_static(C, "x", "default"))  # revealed: int | Literal["default"]
```

## Gradual types

```py
import inspect
from typing import Any

def _(a: Any, tuple_of_any: tuple[Any]):
    reveal_type(inspect.getattr_static(a, "x", "default"))  # revealed: Any | Literal["default"]

    # TODO: Ideally, this would just be `Literal[index]`
    reveal_type(inspect.getattr_static(tuple_of_any, "index", "default"))  # revealed: Literal[index] | Literal["default"]
```

[official documentation]: https://docs.python.org/3/library/inspect.html#inspect.getattr_static
