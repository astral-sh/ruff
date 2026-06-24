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
    reveal_type(unpack("0c", buf))  # revealed: tuple[()]
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
    reveal_type(unpack("@n2xN", buf))  # revealed: tuple[int, int]
    reveal_type(unpack("@P4x", buf))  # revealed: tuple[int]
```

Support for format characters for complex numbers was added in Python 3.14:

```toml
[environment]
python-version = "3.14"
```

```py
from struct import *

def _(buf: bytes):
    reveal_type(unpack("2F", buf))  # revealed: tuple[complex, complex]
    reveal_type(unpack("3D", buf))  # revealed: tuple[complex, complex, complex]
```

## Escape Large Repetition Counts

```py
from struct import *

def _(buf: bytes):
    # 32 is the maximum supported repetition count

    # revealed: tuple[int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int]
    reveal_type(unpack("32i", buf))

    # 33+ repetitions will use a more general fallback type

    # revealed: tuple[Unknown, ...]
    reveal_type(unpack("33i", buf))

    reveal_type(unpack("18446744073709551616c", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("65536i", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("18446744073709551616c@i", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("65536i@i", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("18446744073709551616c2h", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("@n2xN18446744073709551616c@i", buf))  # revealed: tuple[Unknown, ...]
```

## Whitespace in Format Strings

```py
from struct import *

def _(buf: bytes):
    # Whitespace between format specifiers is ignored
    reveal_type(unpack("> b h l", buf))  # revealed: tuple[int, int, int]
    reveal_type(unpack("i i i", buf))  # revealed: tuple[int, int, int]
    reveal_type(unpack("  2h  2i  ", buf))  # revealed: tuple[int, int, int, int]
    reveal_type(unpack("c s", buf))  # revealed: tuple[bytes, bytes]
```

## Unknown Formats

```py
from struct import *

def _(buf: bytes):
    reveal_type(unpack("z", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("10z", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("y", buf))  # revealed: tuple[Unknown, ...]
    reveal_type(unpack("5y", buf))  # revealed: tuple[Unknown, ...]
```
