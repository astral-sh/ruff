# Stepsize zero in slices

We raise a `zero-stepsize-in-slice` diagnostic when a built-in sequence is known to reject a step
size of zero. The check is not exhaustive because custom types and subclasses can decide how to
handle slices in `__getitem__`.

```py
from typing import Any

def builtins(
    values: list[int],
    mutable_bytes: bytearray,
    view: memoryview,
    numbers: range,
    immutable_bytes: bytes,
    text: str,
) -> None:
    values[1:10:0]  # error: [zero-stepsize-in-slice]
    mutable_bytes[1:10:0]  # error: [zero-stepsize-in-slice]
    view[1:10:0]  # error: [zero-stepsize-in-slice]
    numbers[1:10:0]  # error: [zero-stepsize-in-slice]
    immutable_bytes[1:10:0]  # error: [zero-stepsize-in-slice]
    text[1:10:0]  # error: [zero-stepsize-in-slice]

class ZeroSafeList(list[int]):
    def __getitem__(self, key: Any) -> Any:
        return 0

ZeroSafeList()[0:1:0]  # No error

class MySequence:
    def __getitem__(self, s: slice) -> int:
        return 0

MySequence()[0:1:0]  # No error
```
