# Bytes subscripts

## Indexing

```py
b = b"\x00abc\xff"

reveal_type(b[0])  # revealed: Literal[b"\x00"]
reveal_type(b[1])  # revealed: Literal[b"a"]
reveal_type(b[4])  # revealed: Literal[b"\xff"]

reveal_type(b[-1])  # revealed: Literal[b"\xff"]
reveal_type(b[-2])  # revealed: Literal[b"c"]
reveal_type(b[-5])  # revealed: Literal[b"\x00"]

reveal_type(b[False])  # revealed: Literal[b"\x00"]
reveal_type(b[True])  # revealed: Literal[b"a"]

x = b[5]  # error: [index-out-of-bounds] "Index 5 is out of bounds for bytes literal `Literal[b"\x00abc\xff"]` with length 5"
reveal_type(x)  # revealed: Unknown

y = b[-6]  # error: [index-out-of-bounds] "Index -6 is out of bounds for bytes literal `Literal[b"\x00abc\xff"]` with length 5"
reveal_type(y)  # revealed: Unknown

def int_instance() -> int:
    return 42

a = b"abcde"[int_instance()]
# TODO: Support overloads... Should be `bytes`
reveal_type(a)  # revealed: @Todo
```

## Slices

```py
b = b"\x00abc\xff"

reveal_type(b[0:2])  # revealed: Literal[b"\x00a"]
reveal_type(b[-3:])  # revealed: Literal[b"bc\xff"]

b[0:4:0]  # error: [zero-stepsize-in-slice]
b[:4:0]  # error: [zero-stepsize-in-slice]
b[0::0]  # error: [zero-stepsize-in-slice]
b[::0]  # error: [zero-stepsize-in-slice]

def int_instance() -> int: ...

byte_slice1 = b[int_instance() : int_instance()]
# TODO: Support overloads... Should be `bytes`
reveal_type(byte_slice1)  # revealed: @Todo

def bytes_instance() -> bytes: ...

byte_slice2 = bytes_instance()[0:5]
# TODO: Support overloads... Should be `bytes`
reveal_type(byte_slice2)  # revealed: @Todo
```
