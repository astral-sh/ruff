## What it does

Detects nonempty functions that can reach the end of their body when their return type does not
allow `None`.

If a generator annotated with `Generator[YieldType, SendType, ReturnType]` "falls off" the end of
the function in certain code paths, the implicit return type in those code paths will be `None`, so
`ReturnType` must allow `None` in these cases.

## Why is this bad?

When execution reaches the end of a function, Python implicitly returns `None`. A caller relying on
the annotated return type can then fail at runtime.

## Examples

```python
from collections.abc import Generator
from hashlib import sha256
from typing import BinaryIO


# error: [invalid-implicit-return]
def sign(x: int) -> int:
    """Compute the sign of the integer passed in"""
    if x < 0:
        return -1
    if x > 0:
        return 1

    # if `x == 0`, we get to here, and the function implicitly returns `None`.
    # No valid sign exists for `0`, so adjusting the return annotation is appropriate here


# error: [invalid-implicit-return]
def sale_price(price_in_cents: int, *, on_sale: bool) -> int:
    """If `on_sale` is `True`, return half the usual price; otherwise, return the usual price"""
    if on_sale:
        return price_in_cents // 2

    # If `on_sale=False`, we get to here, and the function implicitly returns `None`.
    # This function is missing a `return price_in_cents` statement at the end.


# error: [invalid-implicit-return]
def read_chunks(source: BinaryIO) -> Generator[bytes, None, str]:
    """Yield chunks of bytes and return the file's SHA-256 digest when the stream ends."""
    checksum = sha256()

    while chunk := source.read(8192):
        checksum.update(chunk)
        yield chunk

    # The generator implicitly returns `None` after all chunks have been yielded,
    # contradicting the third type argument of the return-type annotation.
    # Per the docstring, the user meant to add `return checksum.hexdigest()` at the end.
```

Use instead:

```py
from collections.abc import Generator
from hashlib import sha256
from typing import BinaryIO


def sign(x: int) -> int | None:
    """Compute the sign of the integer passed in"""
    if x < 0:
        return -1
    if x > 0:
        return 1
    return None


def sale_price(price_in_cents: int, *, on_sale: bool) -> int:
    """If `on_sale` is `True`, return half the usual price; otherwise, return the usual price"""
    if on_sale:
        return price_in_cents // 2
    return price_in_cents


def read_chunks(source: BinaryIO) -> Generator[bytes, None, str]:
    """Yield chunks of bytes and return the file's SHA-256 digest when the stream ends."""
    checksum = sha256()

    while chunk := source.read(8192):
        checksum.update(chunk)
        yield chunk

    return checksum.hexdigest()
```

## Rule history

- v0.0.101: Split from `invalid-return-type`.
