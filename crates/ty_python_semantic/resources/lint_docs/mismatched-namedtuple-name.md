## What it does

Checks whether the `typename` passed to `typing.NamedTuple` or `collections.namedtuple` differs from
the assigned variable's name.

This rule is only applied if ty infers a string literal type for the `typename` argument.

## Why is this bad?

A mismatched name can be a typo and can make a namedtuple's runtime representation confusing.

## Default level

This rule is disabled by default because namedtuple names are not required to match their assigned
variables, and sometimes the preferred `__repr__` for a namedtuple may include non-identifier
characters.

Enable this rule to enforce consistent namedtuple naming.

## Examples

```python
from collections import namedtuple
from typing import NamedTuple

Point = NamedTuple("Vector", [("x", int)])  # error: [mismatched-namedtuple-name] "Expected "Point", got "Vector""
Pair = namedtuple("Coordinates", "x y")  # error: [mismatched-namedtuple-name] "Expected "Pair", got "Coordinates""
```
