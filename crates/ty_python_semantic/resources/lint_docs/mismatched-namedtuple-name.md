## What it does

Checks whether the `typename` passed to `typing.NamedTuple` or `collections.namedtuple` differs from
the assigned variable.

## Why is this bad?

The `typename` determines the class name shown when printing a named tuple. A different name in the
source makes it harder to identify the class that produced a value.

## Examples

```python
from collections import namedtuple
from typing import NamedTuple

Point = NamedTuple("Vector", [("x", int)])  # error: [mismatched-namedtuple-name]
Pair = namedtuple("Coordinates", "x y")  # error: [mismatched-namedtuple-name]
```
