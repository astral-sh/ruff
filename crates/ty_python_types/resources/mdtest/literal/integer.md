# Integer literals

## Literals

We can infer an integer literal type:

```py
reveal_type(1)  # revealed: Literal[1]
```

## Variable

```py
x = 1
reveal_type(x)  # revealed: Literal[1]
```

## Overflow

We only track integer literals within the range of an i64:

```py
reveal_type(9223372036854775808)  # revealed: int
```

## Big int

We don't support big integer literals; we just infer `int` type instead:

```py
x = 10_000_000_000_000_000_000
reveal_type(x)  # revealed: int
```

## Negated

```py
x = -1
y = -1234567890987654321
z = --987
reveal_type(x)  # revealed: Literal[-1]
reveal_type(y)  # revealed: Literal[-1234567890987654321]
reveal_type(z)  # revealed: Literal[987]
```

## Floats

```py
reveal_type(1.0)  # revealed: float
```

## Complex

```py
reveal_type(2j)  # revealed: complex
```
