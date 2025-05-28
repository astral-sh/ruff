# Bytes subscripts

## Indexing

```py
b = b"\x00abc\xff"

reveal_type(b[0])  # revealed: Literal[0]
reveal_type(b[1])  # revealed: Literal[97]
reveal_type(b[4])  # revealed: Literal[255]

reveal_type(b[-1])  # revealed: Literal[255]
reveal_type(b[-2])  # revealed: Literal[99]
reveal_type(b[-5])  # revealed: Literal[0]

reveal_type(b[False])  # revealed: Literal[0]
reveal_type(b[True])  # revealed: Literal[97]

x = b[5]  # error: [index-out-of-bounds] "Index 5 is out of bounds for bytes literal `Literal[b"\x00abc\xff"]` with length 5"
reveal_type(x)  # revealed: Unknown

y = b[-6]  # error: [index-out-of-bounds] "Index -6 is out of bounds for bytes literal `Literal[b"\x00abc\xff"]` with length 5"
reveal_type(y)  # revealed: Unknown

def _(n: int):
    a = b"abcde"[n]
    reveal_type(a)  # revealed: int
```

## Slices

```py
b: bytes = b"\x00abc\xff"

reveal_type(b[0:2])  # revealed: Literal[b"\x00a"]
reveal_type(b[-3:])  # revealed: Literal[b"bc\xff"]

b[0:4:0]  # error: [zero-stepsize-in-slice]
b[:4:0]  # error: [zero-stepsize-in-slice]
b[0::0]  # error: [zero-stepsize-in-slice]
b[::0]  # error: [zero-stepsize-in-slice]

def _(m: int, n: int):
    byte_slice1 = b[m:n]
    reveal_type(byte_slice1)  # revealed: bytes

def _(s: bytes) -> bytes:
    byte_slice2 = s[0:5]
    return reveal_type(byte_slice2)  # revealed: bytes
```
