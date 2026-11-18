## What it does

Checks that the name passed to the functional form of `TypedDict` matches the variable it is
assigned to. This rule also warns if ty cannot determine the value of a string name argument.

## Why is this bad?

A mismatched name can be a typo and can make a `TypedDict`'s runtime representation confusing.

## Examples

```python
from typing import TypedDict

Movie = TypedDict("Film", {"title": str})  # error: [mismatched-typeddict-name] "Expected "Movie", got "Film""
```
