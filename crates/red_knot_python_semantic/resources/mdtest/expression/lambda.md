# `lambda` expression

## No parameters

`lambda` expressions can be defined without any parameters.

```py
reveal_type(lambda: 1)  # revealed: () -> @Todo(lambda return type)

# error: [unresolved-reference]
reveal_type(lambda: a)  # revealed: () -> @Todo(lambda return type)
```

## With parameters

Unlike parameters in function definition, the parameters in a `lambda` expression cannot be
annotated.

```py
reveal_type(lambda a: a)  # revealed: (a) -> @Todo(lambda return type)
reveal_type(lambda a, b: a + b)  # revealed: (a, b) -> @Todo(lambda return type)
```

But, it can have default values:

```py
reveal_type(lambda a=1: a)  # revealed: (a=Literal[1]) -> @Todo(lambda return type)
reveal_type(lambda a, b=2: a)  # revealed: (a, b=Literal[2]) -> @Todo(lambda return type)
```

And, positional-only parameters:

```py
reveal_type(lambda a, b, /, c: c)  # revealed: (a, b, /, c) -> @Todo(lambda return type)
```

And, keyword-only parameters:

```py
reveal_type(lambda a, *, b=2, c: b)  # revealed: (a, *, b=Literal[2], c) -> @Todo(lambda return type)
```

And, variadic parameter:

```py
reveal_type(lambda *args: args)  # revealed: (*args) -> @Todo(lambda return type)
```

And, keyword-varidic parameter:

```py
reveal_type(lambda **kwargs: kwargs)  # revealed: (**kwargs) -> @Todo(lambda return type)
```

Mixing all of them together:

```py
# revealed: (a, b, /, c=Literal[True], *args, *, d=Literal["default"], e=Literal[5], **kwargs) -> @Todo(lambda return type)
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

Using a variadic paramter:

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
reveal_type(lambda a=lambda x, y: 0: 2)  # revealed: (a=(x, y) -> @Todo(lambda return type)) -> @Todo(lambda return type)
```
