# `string-or-bytes-too-long` (`PYI053`)

```toml
[lint]
select = ["PYI053"]
```

## Long name in `__all__`

Strings in `__all__` correspond to exported names and should be exempt from the rule.

```pyi
__all__ = [
    "aaaaaaaaaabbbbbbbbbbccccccccccddddddddddeeeeeeeeeef",
]
```
