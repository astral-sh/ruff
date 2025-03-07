# `lambda` expression

## No parameters

`lambda` expressions can be defined without any parameters.

```py
reveal_type(lambda: 1)  # revealed: () -> Literal[1]

# error: [unresolved-reference]
reveal_type(lambda: a) # revealed: () -> Unknown
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
reveal_type(lambda a=1: a)  # revealed: (a=Literal[1]) -> Unknown | Literal[1]
reveal_type(lambda a, b=2: a)  # revealed: (a, b=Literal[2]) -> Unknown
```

And, positional-only parameters:

```py
reveal_type(lambda a, b, /, c: c)  # revealed: (a, b, /, c) -> Unknown
```

And, keyword-only parameters:

```py
reveal_type(lambda a, *, b=2, c: b)  # revealed: (a, *, b=Literal[2], c) -> Unknown | Literal[2]
```

And, variadic parameter:

```py
# TODO: should be `tuple[Unknown, ...]` (needs generics)
reveal_type(lambda *args: args)  # revealed: (*args) -> tuple
```

And, keyword-varidic parameter:

```py
# TODO: should be `dict[str, Unknown]` (needs generics)
reveal_type(lambda **kwargs: kwargs)  # revealed: (**kwargs) -> dict
```

Mixing all of them together:

```py
# revealed: (a, b, /, c=Literal[True], *args, *, d=Literal["default"], e=Literal[5], **kwargs) -> None
reveal_type(lambda a, b, /, c=True, *args, d="default", e=5, **kwargs: None)
```
