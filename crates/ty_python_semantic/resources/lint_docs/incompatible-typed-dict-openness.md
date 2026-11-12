## What it does

Detects `TypedDict` subclasses that violate a base class's restrictions on entries whose keys the
base does not declare. This can happen when a subclass declares a new key or changes the `closed` or
`extra_items` setting.

## Why is this bad?

Code that accepts a closed `TypedDict` can assume that only the declared keys are present. If a base
class allows read-only extra items, code can read those items as the declared type. A subclass that
widens the type would break that guarantee. If the extra items are mutable, code can also write
values of the declared type, so a subclass must preserve both the type and mutability.

## Example

```python
from typing_extensions import ReadOnly, TypedDict


class Scores(TypedDict, extra_items=ReadOnly[int]): ...


# error: [incompatible-typed-dict-openness]
class MixedScores(Scores, extra_items=ReadOnly[int | str]): ...
```

## Rule history

- v0.0.102: Split out from `invalid-typed-dict-header`.
