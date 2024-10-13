# Variables

## Union resolution

```py
if flag:
    x = 1
else:
    x = 2
reveal_type(x)  # revealed: Literal[1, 2]
```

## Assignment

### Walrus operator

```py
x = (y := 1) + 1
reveal_type(x)  # revealed: Literal[2]
reveal_type(y)  # revealed: Literal[1]
```

### Walrus self-addition

```py
x = 0
(x := x + 1)
reveal_type(x)  # revealed: Literal[1]
```

### Multi-target assignment

```py
x = y = 1
reveal_type(x)  # revealed: Literal[1]
reveal_type(y)  # revealed: Literal[1]
```

### Maybe unbound

```py
if flag:
    y = 3
x = y
reveal_type(x)  # revealed: Unbound | Literal[3]
```

### Unbound

```py
x = foo; foo = 1
reveal_type(x)  # revealed: Unbound
```

### Annotation only transparent to local inference

```py
x = 1
x: int
y = x

reveal_type(y)  # revealed: Literal[1]
```

### Violates own annotation

```py
x: int = 'foo' # error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"

```

### Violates previous annotation

```py
x: int
x = 'foo'  # error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
```

### Shadowing declaration is OK

This test ensures that redeclaring a variable with a different type is allowed:

```py
x: str = 'foo'
x: int = 1
```

### Violates previous assignment

```py
x = 1
x: str  # error: [invalid-declaration] "Cannot declare type `str` for inferred type `Literal[1]`"
```

### Incompatible declarations

```py
if flag:
    x: str
else:
    x: int
x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: str, int"
```

### Partial declarations

```py
if flag:
    x: int
x = 1  # error: [conflicting-declarations] "Conflicting declared types for `x`: Unknown, int"
```

### Incompatible declarations with bad assignment

```py
if flag:
    x: str
else:
    x: int

# error: [conflicting-declarations]
# error: [invalid-assignment]
x = b'foo'
```

### Shadow after incompatible declarations is OK

```py
if flag:
    x: str
else:
    x: int
x: bytes = b'foo'
```
