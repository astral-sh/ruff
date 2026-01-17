# Struct Unpacking

This test verifies the correct unpacking of Python structs using the `struct` module. It checks
various formats and ensures that the unpacked values match the expected results.

## Simple Unpacking Tests

```py
from struct import *

def _(buf: bytes):
    reveal_type(unpack(">bhl", buf))  # revealed: tuple[int, int, int]
    reveal_type(unpack("2xH", buf))  # revealed: tuple[int]
    reveal_type(unpack("@i", buf))  # revealed: tuple[int]
    reveal_type(unpack("=i", buf))  # revealed: tuple[int]
    reveal_type(unpack("c3x", buf))  # revealed: tuple[bytes]
    reveal_type(unpack("2c", buf))  # revealed: tuple[bytes, bytes]
    reveal_type(unpack("5c", buf))  # revealed: tuple[bytes, bytes, bytes, bytes, bytes]
    reveal_type(unpack("0s", buf))  # revealed: tuple[bytes]
    reveal_type(unpack("1s", buf))  # revealed: tuple[bytes]
    reveal_type(unpack("255s", buf))  # revealed: tuple[bytes]
    reveal_type(unpack("e", buf))  # revealed: tuple[float]
    reveal_type(unpack("2e", buf))  # revealed: tuple[float, float]
    reveal_type(unpack("e4x", buf))  # revealed: tuple[float]
    reveal_type(unpack("3eH", buf))  # revealed: tuple[float, float, float, int]
    reveal_type(unpack("?x?", buf))  # revealed: tuple[bool, bool]
    reveal_type(unpack("2?", buf))  # revealed: tuple[bool, bool]
    reveal_type(unpack("?2xI", buf))  # revealed: tuple[bool, int]
    reveal_type(unpack("fd4x", buf))  # revealed: tuple[float, float]
    reveal_type(unpack("d2xH", buf))  # revealed: tuple[float, int]
    reveal_type(unpack("2i4x2h", buf))  # revealed: tuple[int, int, int, int]
    reveal_type(unpack("iP", buf))  # revealed: tuple[int, int]
    reveal_type(unpack(">n2xN", buf))  # revealed: tuple[int, int]
    reveal_type(unpack("!P4x", buf))  # revealed: tuple[int]
```

## Escape Large Repetition Counts

```py
from struct import *

def _(buf: bytes):
    reveal_type(unpack("18446744073709551616c", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("65536i", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("18446744073709551616c@i", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("65536i@i", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("18446744073709551616c2h", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack(">n2xN18446744073709551616c@i", buf))  # revealed: tuple[Unknown, ...]
```
