# Tuple subscripts

## Basic

```py
t = (1, "a", "b")

reveal_type(t[0])  # revealed: Literal[1]
reveal_type(t[1])  # revealed: Literal["a"]
reveal_type(t[-1])  # revealed: Literal["b"]
reveal_type(t[-2])  # revealed: Literal["a"]

a = t[4]  # error: [index-out-of-bounds]
reveal_type(a)  # revealed: Unknown

b = t[-4]  # error: [index-out-of-bounds]
reveal_type(b)  # revealed: Unknown
```
