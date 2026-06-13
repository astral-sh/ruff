# Stepsize zero in slices

We raise a `zero-stepsize-in-slice` diagnostic when trying to slice a literal string, bytes, tuple,
or a `list` with a step size of zero (see tests in `string.md`, `bytes.md` and `tuple.md`). A `list`
type includes exact `list` instances that reject it, even though subclasses can override
`__getitem__`. But we don't want to raise this diagnostic when slicing a custom type, including such
a subclass:

```py
from typing import Any

values = list(range(10))
values[1:10:0]  # error: [zero-stepsize-in-slice]

class ZeroSafeList(list[int]):
    def __getitem__(self, key: Any) -> Any:
        return 0

ZeroSafeList()[0:1:0]  # No error

class MySequence:
    def __getitem__(self, s: slice) -> int:
        return 0

MySequence()[0:1:0]  # No error
```
