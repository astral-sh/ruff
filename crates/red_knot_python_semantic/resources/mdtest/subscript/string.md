# String subscripts

## Indexing

```py
s = "abcde"

reveal_type(s[0])  # revealed: Literal["a"]
reveal_type(s[1])  # revealed: Literal["b"]
reveal_type(s[-1])  # revealed: Literal["e"]
reveal_type(s[-2])  # revealed: Literal["d"]

reveal_type(s[False])  # revealed: Literal["a"]
reveal_type(s[True])  # revealed: Literal["b"]

a = s[8]  # error: [index-out-of-bounds] "Index 8 is out of bounds for string `Literal["abcde"]` with length 5"
reveal_type(a)  # revealed: Unknown

b = s[-8]  # error: [index-out-of-bounds] "Index -8 is out of bounds for string `Literal["abcde"]` with length 5"
reveal_type(b)  # revealed: Unknown
```

## Slices

```py
s = "abcde"

reveal_type(s[0:0])  # revealed: Literal[""]
reveal_type(s[0:1])  # revealed: Literal["a"]
reveal_type(s[0:2])  # revealed: Literal["ab"]
reveal_type(s[0:5])  # revealed: Literal["abcde"]

reveal_type(s[-3:5])  # revealed: Literal["cde"]

reveal_type(s[0:])  # revealed: Literal["abcde"]
reveal_type(s[1:])  # revealed: Literal["bcde"]
reveal_type(s[5:])  # revealed: Literal[""]

reveal_type(s[:0])  # revealed: Literal[""]
reveal_type(s[:1])  # revealed: Literal["a"]
reveal_type(s[:5])  # revealed: Literal["abcde"]

reveal_type(s[:])  # revealed: Literal["abcde"]

a = s[0:5:0]  # error: [slice-step-zero]
reveal_type(a)  # revealed: Unknown
```

## Function return

```py
def int_instance() -> int:
    return 42

a = "abcde"[int_instance()]
# TODO: Support overloads... Should be `str`
reveal_type(a)  # revealed: @Todo
```
