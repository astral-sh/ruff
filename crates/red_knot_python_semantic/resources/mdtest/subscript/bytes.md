# Bytes subscript

## Simple

```py
w = b'red' b'knot'
x = b'hello'
y = b'world' + b'!'
z = b'\xff\x00'

reveal_type(w)  # revealed: Literal[b"redknot"]
reveal_type(x)  # revealed: Literal[b"hello"]
reveal_type(y)  # revealed: Literal[b"world!"]
reveal_type(z)  # revealed: Literal[b"\xff\x00"]
```
