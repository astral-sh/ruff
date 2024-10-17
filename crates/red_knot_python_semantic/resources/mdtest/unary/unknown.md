## Unary Addition

```py
# error: [non-subscriptable] "Cannot subscript object of type `Literal[1]` with no `__getitem__` method"
a = 1[0]

reveal_type(+a) # revealed: Unknown
```

## Unary Subtraction

```py
# error: [non-subscriptable] "Cannot subscript object of type `Literal[1]` with no `__getitem__` method"
a = 1[0]

reveal_type(~a) # revealed: Unknown
```

## Unary Bitwise Inversion

```py
# error: [non-subscriptable] "Cannot subscript object of type `Literal[1]` with no `__getitem__` method"
a = 1[0]

reveal_type(~a) # revealed: Unknown
```
