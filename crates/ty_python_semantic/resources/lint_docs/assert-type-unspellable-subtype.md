## What it does

Checks for `assert_type()` calls where the actual type
is an unspellable subtype of the asserted type.

## Why is this bad?

`assert_type()` is intended to ensure that the inferred type of a value
is exactly the same as the asserted type. But in some situations, ty
has nonstandard extensions to the type system that allow it to infer
more precise types than can be expressed in user annotations. ty emits a
different error code to `type-assertion-failure` in these situations so
that users can easily differentiate between the two cases.

## Example

```toml
[environment]
python-version = "3.11"
```

```python
from typing import assert_type


def _(x: int):
    assert_type(x, int)  # fine
    if x:
        # the actual type is `int & ~AlwaysFalsy`,
        # which excludes types like `Literal[0]`
        # error: [assert-type-unspellable-subtype]
        assert_type(x, int)
```
