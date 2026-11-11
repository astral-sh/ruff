## What it does

Checks for classes that inherit conflicting specializations of the same generic base class.

## Why is this bad?

Callers can use a subclass through either of its bases. Conflicting type arguments can give them
incompatible expectations for the same inherited attribute.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
class Box[T]:
    value: T


class IntBox(Box[int]): ...


class StrBox(Box[str]): ...


class MixedBox(IntBox, StrBox): ...  # error: [inconsistent-generic-bases]
```

A function accepting `IntBox` can write an integer to the `value` attribute of a `MixedBox`
instance. A function accepting `StrBox` would then read that integer as a `str`.

## Rule history

- v0.0.102: Split out from `invalid-generic-class`.
