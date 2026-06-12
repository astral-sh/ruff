## What it does

Makes sure that the argument of `static_assert` is statically known to be true.

## Why is this bad?

A `static_assert` call represents an explicit request from the user
for the type checker to emit an error if the argument cannot be verified
to evaluate to `True` in a boolean context.

## Examples

```python
from ty_extensions import static_assert

# evaluates to `False`
static_assert(1 + 1 == 3)  # error

# does not have a statically known truthiness
static_assert(int(2.0 * 3.0) == 6)  # error
```
