# `typing.Required` and `typing.NotRequired`

[`typing.Required`] and [`typing.NotRequired`] are type qualifiers used to override the default
requiredness of individual fields in a `TypedDict` class.

## Valid usage in `TypedDict`

`Required` and `NotRequired` are only valid inside `TypedDict` class bodies:

```py
from typing_extensions import TypedDict, Required, NotRequired

class Movie(TypedDict, total=False):
    title: Required[str]
    year: Required[int]
    director: NotRequired[str]
```

## Invalid usage outside `TypedDict`

`Required` and `NotRequired` are not allowed outside of `TypedDict` class bodies.

### In a regular class

```py
from typing_extensions import Required, NotRequired

class MyClass:
    # error: [invalid-type-form] "`Required` is only allowed in TypedDict fields"
    x: Required[int]
    # error: [invalid-type-form] "`NotRequired` is only allowed in TypedDict fields"
    y: NotRequired[str]
```

### At module level

```py
from typing_extensions import Required, NotRequired

# error: [invalid-type-form] "`Required` is only allowed in TypedDict fields"
x: Required[int]
# error: [invalid-type-form] "`NotRequired` is only allowed in TypedDict fields"
y: NotRequired[str]
```

### In a function body

```py
from typing_extensions import Required, NotRequired

def f():
    # error: [invalid-type-form] "`Required` is only allowed in TypedDict fields"
    x: Required[int] = 1
    # error: [invalid-type-form] "`NotRequired` is only allowed in TypedDict fields"
    y: NotRequired[str] = ""
```

## Nested `Required` and `NotRequired`

`Required` and `NotRequired` cannot be nested inside each other:

```py
from typing_extensions import TypedDict, Required, NotRequired

class TD(TypedDict):
    # error: [invalid-type-form] "`typing.Required` cannot be nested inside `Required` or `NotRequired`"
    a: Required[Required[int]]
    # error: [invalid-type-form] "`typing.NotRequired` cannot be nested inside `Required` or `NotRequired`"
    b: NotRequired[NotRequired[int]]
    # error: [invalid-type-form] "`typing.Required` cannot be nested inside `Required` or `NotRequired`"
    c: Required[NotRequired[int]]
    # error: [invalid-type-form] "`typing.NotRequired` cannot be nested inside `Required` or `NotRequired`"
    d: NotRequired[Required[int]]
```

[`typing.Required`]: https://docs.python.org/3/library/typing.html#typing.Required
[`typing.NotRequired`]: https://docs.python.org/3/library/typing.html#typing.NotRequired
