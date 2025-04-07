# `lambda` expression

## No parameters

`lambda` expressions can be defined without any parameters.

```py
reveal_type(lambda: 1)  # revealed: () -> Unknown

# error: [unresolved-reference]
reveal_type(lambda: a)  # revealed: () -> Unknown
```

## With parameters

Unlike parameters in function definition, the parameters in a `lambda` expression cannot be
annotated.

```py
reveal_type(lambda a: a)  # revealed: (a) -> Unknown
reveal_type(lambda a, b: a + b)  # revealed: (a, b) -> Unknown
```

But, it can have default values:

```py
reveal_type(lambda a=1: a)  # revealed: (a=Literal[1]) -> Unknown
reveal_type(lambda a, b=2: a)  # revealed: (a, b=Literal[2]) -> Unknown
```

And, positional-only parameters:

```py
reveal_type(lambda a, b, /, c: c)  # revealed: (a, b, /, c) -> Unknown
```

And, keyword-only parameters:

```py
reveal_type(lambda a, *, b=2, c: b)  # revealed: (a, *, b=Literal[2], c) -> Unknown
```

And, variadic parameter:

```py
reveal_type(lambda *args: args)  # revealed: (*args) -> Unknown
```

And, keyword-varidic parameter:

```py
reveal_type(lambda **kwargs: kwargs)  # revealed: (**kwargs) -> Unknown
```

Mixing all of them together:

```py
# revealed: (a, b, /, c=Literal[True], *args, *, d=Literal["default"], e=Literal[5], **kwargs) -> Unknown
reveal_type(lambda a, b, /, c=True, *args, d="default", e=5, **kwargs: None)
```

## Parameter type

In addition to correctly inferring the `lambda` expression, the parameters should also be inferred
correctly.

Using a parameter with no default value:

```py
lambda x: reveal_type(x)  # revealed: Unknown
```

Using a parameter with default value:

```py
lambda x=1: reveal_type(x)  # revealed: Unknown | Literal[1]
```

Using a variadic parameter:

```py
# TODO: should be `tuple[Unknown, ...]` (needs generics)
lambda *args: reveal_type(args)  # revealed: tuple
```

Using a keyword-varidic parameter:

```py
# TODO: should be `dict[str, Unknown]` (needs generics)
lambda **kwargs: reveal_type(kwargs)  # revealed: dict
```

## Nested `lambda` expressions

Here, a `lambda` expression is used as the default value for a parameter in another `lambda`
expression.

```py
reveal_type(lambda a=lambda x, y: 0: 2)  # revealed: (a=(x, y) -> Unknown) -> Unknown
```

## Assignment

This does not enumerate all combinations of parameter kinds as that should be covered by the
[subtype tests for callable types](./../type_properties/is_subtype_of.md#callable).

```py
from typing import Callable

a1: Callable[[], None] = lambda: None
a2: Callable[[int], None] = lambda x: None
a3: Callable[[int, int], None] = lambda x, y, z=1: None
a4: Callable[[int, int], None] = lambda *args: None

# error: [invalid-assignment]
a5: Callable[[], None] = lambda x: None
# error: [invalid-assignment]
a6: Callable[[int], None] = lambda: None
```
