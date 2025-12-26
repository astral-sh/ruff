# Comparison: Byte literals

These tests assert that we infer precise `Literal` types for comparisons between objects inferred as
having `Literal` bytes types:

```py
reveal_type(b"abc" == b"abc")  # revealed: Literal[True]
reveal_type(b"abc" == b"ab")  # revealed: Literal[False]

reveal_type(b"abc" != b"abc")  # revealed: Literal[False]
reveal_type(b"abc" != b"ab")  # revealed: Literal[True]

reveal_type(b"abc" < b"abd")  # revealed: Literal[True]
reveal_type(b"abc" < b"abb")  # revealed: Literal[False]

reveal_type(b"abc" <= b"abc")  # revealed: Literal[True]
reveal_type(b"abc" <= b"abb")  # revealed: Literal[False]

reveal_type(b"abc" > b"abd")  # revealed: Literal[False]
reveal_type(b"abc" > b"abb")  # revealed: Literal[True]

reveal_type(b"abc" >= b"abc")  # revealed: Literal[True]
reveal_type(b"abc" >= b"abd")  # revealed: Literal[False]

reveal_type(b"" in b"")  # revealed: Literal[True]
reveal_type(b"" in b"abc")  # revealed: Literal[True]
reveal_type(b"abc" in b"")  # revealed: Literal[False]
reveal_type(b"ab" in b"abc")  # revealed: Literal[True]
reveal_type(b"abc" in b"abc")  # revealed: Literal[True]
reveal_type(b"d" in b"abc")  # revealed: Literal[False]
reveal_type(b"ac" in b"abc")  # revealed: Literal[False]
reveal_type(b"\x81\x82" in b"\x80\x81\x82")  # revealed: Literal[True]
reveal_type(b"\x82\x83" in b"\x80\x81\x82")  # revealed: Literal[False]

reveal_type(b"ab" not in b"abc")  # revealed: Literal[False]
reveal_type(b"ac" not in b"abc")  # revealed: Literal[True]

reveal_type(b"abc" is b"abc")  # revealed: bool
reveal_type(b"abc" is b"ab")  # revealed: Literal[False]

reveal_type(b"abc" is not b"abc")  # revealed: bool
reveal_type(b"abc" is not b"ab")  # revealed: Literal[True]
```
