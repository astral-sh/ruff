# Bytes literals

## Simple

```py
reveal_type(b"t" b"y")  # revealed: Literal[b"ty"]
reveal_type(b"hello")  # revealed: Literal[b"hello"]
reveal_type(b"world" + b"!")  # revealed: Literal[b"world!"]
reveal_type(b"\xff\x00")  # revealed: Literal[b"\xff\x00"]
```
