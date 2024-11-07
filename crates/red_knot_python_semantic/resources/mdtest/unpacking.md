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
# TODO: Add diagnostic (there aren't enough values to unpack)
(a, b, c) = (1, 2)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# TODO: Add diagnostic (too many values to unpack)
(a, b) = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
```

### Starred expression (1)

```py
# TODO: Add diagnostic (need more values to unpack)
[a, *b, c, d] = (1, 2)
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be list[Any] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: Literal[2]
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
[a, *b, c] = (1, 2)
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be list[Any] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: Literal[2]
```

### Starred expression (3)

```py
[a, *b, c] = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: Literal[3]
```

### Starred expression (4)

```py
[a, *b, c, d] = (1, 2, 3, 4, 5, 6)
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: Literal[5]
reveal_type(d)  # revealed: Literal[6]
```

### Starred expression (5)

```py
[a, b, *c] = (1, 2, 3, 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(c)  # revealed: @Todo
```

### Starred expression (6)

```py
# TODO: Add diagnostic (need more values to unpack)
(a, b, c, *d, e, f) = (1,)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: @Todo
reveal_type(e)  # revealed: Unknown
reveal_type(f)  # revealed: Unknown
```

### Non-iterable unpacking

```py
# error: "Object of type `Literal[1]` is not iterable"
a, b = 1
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Custom iterator unpacking

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

(a, b) = Iterable()
reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: int
```

### Custom iterator unpacking nested

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

(a, (b, c), d) = (1, Iterable(), 2)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: int
reveal_type(c)  # revealed: int
reveal_type(d)  # revealed: Literal[2]
```

## String

### Simple unpacking

```py
a, b = "ab"
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
```

### Uneven unpacking (1)

```py
# TODO: Add diagnostic (there aren't enough values to unpack)
a, b, c = "ab"
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# TODO: Add diagnostic (too many values to unpack)
a, b = "abc"
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
```

### Starred expression (1)

```py
# TODO: Add diagnostic (need more values to unpack)
(a, *b, c, d) = "ab"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be list[LiteralString] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: LiteralString
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
(a, *b, c) = "ab"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be list[Any] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: LiteralString
```

### Starred expression (3)

```py
(a, *b, c) = "abc"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be list[LiteralString] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: LiteralString
```

### Starred expression (4)

```py
(a, *b, c, d) = "abcdef"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be list[LiteralString] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo
reveal_type(c)  # revealed: LiteralString
reveal_type(d)  # revealed: LiteralString
```

### Starred expression (5)

```py
(a, b, *c) = "abcd"
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(c)  # revealed: @Todo
```
