## What it does

Checks whether the `typename` passed to `typing.NamedTuple` or `collections.namedtuple` differs from
the assigned variable's name.

This rule is only applied if ty infers a string literal type for the `typename` argument.

## Why is this bad?

A mismatched name can be a typo and can make a namedtuple's runtime representation confusing.

## Default level

This rule is disabled by default because namedtuple names are not required to match their assigned
variables.

Enable this rule to enforce consistent namedtuple naming.

## Examples

```python
from collections import namedtuple
from typing import NamedTuple

# error: [mismatched-namedtuple-name] "Expected "Point", got "Vector""
Point = NamedTuple("Vector", [("x", int)])
# error: [mismatched-namedtuple-name] "Expected "Pair", got "Coordinates""
Pair = namedtuple("Coordinates", "x y")
```

## See also

`invalid-named-tuple` is emitted on namedtuples that will actually lead to exceptions being raised
at runtime, due to a `typename` that is either an invalid identifier or a keyword.

## Rule history

- v0.0.108: Split out from `mismatched-type-name`.
