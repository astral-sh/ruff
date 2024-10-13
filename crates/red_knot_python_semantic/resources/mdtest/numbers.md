# Numbers

## Integers

### Literals

We can infer an integer literal type:

```py
reveal_type(1)  # revealed: Literal[1]
```

### Variable

We can track that variable as an integer:

```py
x = 1
reveal_type(x)  # revealed: Literal[1]
```

### Overflow

We only track integer literals within the range of an i64:

```py
reveal_type(9223372036854775808)  # revealed: int
```

### Big int

We can track that a big int literal resolves to the corresponding integer literal:

```py
x = 10_000_000_000_000_000_000
reveal_type(x)  # revealed: int
```

### Negated

We can track that a negated integer literal resolves to the corresponding integer literal:

```py
x = -1
y = -1234567890987654321
z = --987
reveal_type(x)  # revealed: Literal[-1]
reveal_type(y)  # revealed: Literal[-1234567890987654321]
reveal_type(z)  # revealed: Literal[987]
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

## Arithmetic

### Basic Arithmetic

```py
a = 2 + 1
b = a - 4
c = a * b
d = c // 3
e = c / 3
f = 5 % 3

reveal_type(a)  # revealed: Literal[3]
reveal_type(b)  # revealed: Literal[-1]
reveal_type(c)  # revealed: Literal[-3]
reveal_type(d)  # revealed: Literal[-1]
reveal_type(e)  # revealed: float
reveal_type(f)  # revealed: Literal[2]
```

### TODO: Division by Zero

> TODO: `a` should be `int` and `e` should be `float` once we support inference.

```py
a = 1 / 0  # error: "Cannot divide object of type `Literal[1]` by zero"
b = 2 // 0  # error: "Cannot floor divide object of type `Literal[2]` by zero"
c = 3 % 0  # error: "Cannot reduce object of type `Literal[3]` modulo zero"
d = int() / 0  # error: "Cannot divide object of type `int` by zero"
e = 1.0 / 0  # error: "Cannot divide object of type `float` by zero"

reveal_type(a)  # revealed: float
reveal_type(b)  # revealed: int
reveal_type(c)  # revealed: int
reveal_type(d)  # revealed: @Todo
reveal_type(e)  # revealed: @Todo
```

## Unary Operations

### Unary Addition

```py
a = +0
b = +1
c = +True

reveal_type(a)  # revealed: Literal[0]
reveal_type(b)  # revealed: Literal[1]
reveal_type(c)  # revealed: Literal[1]
```

### Unary Subtraction

```py
a = -0
b = -1
c = -True

reveal_type(a)  # revealed: Literal[0]
reveal_type(b)  # revealed: Literal[-1]
reveal_type(c)  # revealed: Literal[-1]
```

### Unary Bitwise Inversion

```py
a = ~0
b = ~1
c = ~True

reveal_type(a) # revealed: Literal[-1] 
reveal_type(b) # revealed: Literal[-2]
reveal_type(c) # revealed: Literal[-2]
```
