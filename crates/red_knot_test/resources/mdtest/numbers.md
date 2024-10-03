# Numbers

## Integers

### Literals

We can infer an integer literal type:

```py
reveal_type(1)  # Type: Literal[1]
```

### Overflow

We only track integer literals within the range of an i64:

```py
reveal_type(9223372036854775808)  # Type: int
```

## Floats

There aren't literal float types, but we infer the general float type:

```py
reveal_type(1.0)  # Type: float
```

## Complex

Same for complex:

```py
reveal_type(2j)  # Type: complex
```
