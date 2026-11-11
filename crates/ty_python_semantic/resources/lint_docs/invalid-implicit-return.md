## What it does

Detects nonempty functions that can reach the end of their body and implicitly return `None`,
despite the fact that their return-type annotations do not allow `None` to be returned.

This rule only covers functions with non-empty bodies. Functions with empty "stub" bodies are
covered by the separate `invalid-implicit-return-in-function-with-stub-body` rule.

## Why is this bad?

When execution reaches the end of a function, Python implicitly returns `None`. If the return-type
annotation does not permit `None` to be returned, a caller relying on the annotated return type can
then fail at runtime.

For a regular non-generator function, an implicit return is only allowed if the return-type
annotation is `None` or a supertype of `None`. For a synchronous generator function annotated with
`Generator[YieldType, SendType, ReturnType]` or similar, an implicit return is only allowed if
`ReturnType` is `None` or a supertype of `None`.

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

- v0.0.0-alpha.2: First added to ty as part of the `invalid-return-type` diagnostic (since renamed
    to `invalid-explicit-return`).
- v0.0.101: Split out from `invalid-return-type` as a standalone rule.
