## What it does

Detects `yield` and `yield from` expressions where the "yield" or "send" type
is incompatible with the generator function's annotated return type.

## Why is this bad?

Yielding a value of a type that doesn't match the generator's declared yield type,
or using `yield from` with a sub-iterator whose yield or send type is incompatible,
is a type error that may cause downstream consumers of the generator to receive
values of an unexpected type.

## Examples

```python
from typing import Iterator


def gen() -> Iterator[int]:
    yield "not an int"  # error: [invalid-yield]
```
