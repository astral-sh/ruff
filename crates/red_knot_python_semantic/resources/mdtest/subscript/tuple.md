# Tuple subscripts

## Basic

```py
t = (1, 'a', 'b')

a = t[0]
b = t[1]
c = t[-1]
d = t[-2]
e = t[4]        # error: [index-out-of-bounds]
f = t[-4]       # error: [index-out-of-bounds]

reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal["a"]
reveal_type(c)  # revealed: Literal["b"]
reveal_type(d)  # revealed: Literal["a"]
reveal_type(e)  # revealed: Unknown
reveal_type(f)  # revealed: Unknown
```
