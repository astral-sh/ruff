# Numbers

## Integers

### Literals

We can infer an integer literal type:

```py
reveal_type(1)  # revealed: Literal[1]
```

We can track that variable as an integer:

```py path=a.py
x = 1
reveal_type(x)  # revealed: Literal[1]
```

### Overflow

We only track integer literals within the range of an i64:

```py
reveal_type(9223372036854775808)  # revealed: int
```

## Floats

There aren't literal float types, but we infer the general float type:

```py
reveal_type(1.0)  # revealed: float
```

## Complex

Same for complex:

```py
reveal_type(2j)  # revealed: complex
```
