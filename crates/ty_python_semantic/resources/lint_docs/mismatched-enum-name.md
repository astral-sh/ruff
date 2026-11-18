## What it does

Checks whether the class name passed to a functional enum constructor, such as `Enum`, `IntEnum`, or
`Flag`, differs from the assigned variable's name.

This rule is only applied if ty infers a string literal type for the class name argument.

## Why is this bad?

A mismatched name can be a typo and can make a enum's runtime representation confusing.

## Default level

This rule is disabled by default because enum names are not required to match their assigned
variables, and sometimes the preferred `__repr__` for an enum may include non-identifier
characters.

Enable this rule to enforce consistent enum naming.

## Examples

```python
from enum import Enum

Color = Enum("Colour", "RED BLUE")  # error: [mismatched-enum-name] "Expected "Color", got "Colour""
```
