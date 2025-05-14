# Stepsize zero in slices

We raise a `zero-stepsize-in-slice` diagnostic when trying to slice a literal string, bytes, or
tuple with a step size of zero (see tests in `string.md`, `bytes.md` and `tuple.md`). But we don't
want to raise this diagnostic when slicing a custom type:

```py
class MySequence:
    def __getitem__(self, s: slice) -> int:
        return 0

MySequence()[0:1:0]  # No error
```
