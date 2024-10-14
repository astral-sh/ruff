# Unpacking

## Tuple

### Simple tuple

```py
(a, b, c) = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
```

### Simple list

```py
[a, b, c] = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
```

### Simple mixed

```py
[a, (b, c), d] = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
reveal_type(d)  # revealed: Literal[4]
```

### Multiple assignment

```py
a, b = c = 1, 2
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: tuple[Literal[1], Literal[2]]
```

### Nested tuple with unpacking

```py
(a, (b, c), d) = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
reveal_type(d)  # revealed: Literal[4]
```

### Nested tuple without unpacking

```py
(a, b, c) = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: tuple[Literal[2], Literal[3]]
reveal_type(c)  # revealed: Literal[4]
```

### Uneven unpacking (1)

```py
(a, b, c) = (1, 2)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
(a, b) = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
```

### Starred expression (1)

TODO: The error message here is because `infer_starred_expression` isn't complete

```py
[a, *b, c, d] = (1, 2)  # error: "Object of type `None` is not iterable"
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be List[int]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

TODO: The error message here is because `infer_starred_expression` isn't complete

```py
[a, *b, c] = (1, 2, 3)  # error: "Object of type `None` is not iterable"
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be List[int]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Literal[3]
```

### Starred expression (3)

TODO: The error message here is because `infer_starred_expression` isn't complete

```py
[a, *b, c, d] = (1, 2, 3, 4, 5, 6)  # error: "Object of type `None` is not iterable"
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be List[int]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Literal[5]
reveal_type(d)  # revealed: Literal[6]
```

### Non-iterable unpacking

TODO: Remove duplicate diagnostics. This is happening because for a sequence-like
assignment target, multiple definitions are created and the inference engine runs
on each of them which results in duplicate diagnostics.

```py
# error: "Object of type `Literal[1]` is not iterable"
# error: "Object of type `Literal[1]` is not iterable"
a, b = 1
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

## String

### Simple unpacking

```py
a, b = 'abc'
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
```

### Starred expression

TODO: The error message here is because `infer_starred_expression` isn't complete

```py
a, *b = 'abc'  # error: "Object of type `None` is not iterable"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be List[str]
reveal_type(b)  # revealed: Unknown
```
