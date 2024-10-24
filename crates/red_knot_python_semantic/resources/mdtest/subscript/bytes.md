# Bytes subscript

## Simple

```py
reveal_type(b"red" b"knot")  # revealed: Literal[b"redknot"]
reveal_type(b"hello")  # revealed: Literal[b"hello"]
reveal_type(b"world" + b"!")  # revealed: Literal[b"world!"]
reveal_type(b"\xff\x00")  # revealed: Literal[b"\xff\x00"]
```

## Indexing

```py
b = b"\x00abc\xff"

reveal_type(b[0])  # revealed: Literal[b"\x00"]
reveal_type(b[1])  # revealed: Literal[b"a"]
reveal_type(b[-1])  # revealed: Literal[b"\xff"]
reveal_type(b[-2])  # revealed: Literal[b"c"]

reveal_type(b[False])  # revealed: Literal[b"\x00"]
reveal_type(b[True])  # revealed: Literal[b"a"]

x = b[5]  # error: [index-out-of-bounds] "Index 5 is out of bounds for bytes literal `Literal[b"\x00abc\xff"]` with length 5"
reveal_type(x)  # revealed: Unknown

y = b[-6]  # error: [index-out-of-bounds] "Index -6 is out of bounds for bytes literal `Literal[b"\x00abc\xff"]` with length 5"
reveal_type(y)  # revealed: Unknown
```
