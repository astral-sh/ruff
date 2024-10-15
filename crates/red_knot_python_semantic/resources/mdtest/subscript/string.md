# Subscript on strings

## Simple

```py
s = 'abcde'

a = s[0]
b = s[1]
c = s[-1]
d = s[-2]
e = s[8]        # error: [index-out-of-bounds] "Index 8 is out of bounds for string `Literal["abcde"]` with length 5"
f = s[-8]       # error: [index-out-of-bounds] "Index -8 is out of bounds for string `Literal["abcde"]` with length 5"

reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: Literal["b"]
reveal_type(c)  # revealed: Literal["e"]
reveal_type(d)  # revealed: Literal["d"]
reveal_type(e)  # revealed: Unknown
reveal_type(f)  # revealed: Unknown
```

## Function return

```py
def add(x: int, y: int) -> int:
    return x + y

a = 'abcde'[add(0, 1)]
reveal_type(a)  # revealed: str
```
