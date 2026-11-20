## What it does

Checks whether the name passed to `TypeVarTuple` differs from the assigned variable, or if ty cannot
determine the value of a string passed to the `name` argument.

## Why is this bad?

A mismatched name can be a typo and can make a `TypeVarTuple`'s runtime representation confusing.

## Examples

```python
from typing_extensions import TypeVarTuple

# error: "Expected "Ts", got "Us""
Ts = TypeVarTuple("Us")


def f(x: str):
    # error: "Cannot verify that a value of type `str` matches "Us""
    Us = TypeVarTuple(x)
```

## Fix safety

This rule's autofix is usually marked as safe. ty marks it as unsafe if either of the following
conditions holds:

- The definition is outside module scope in a non-stub file. Changing the name can break pickling in
    these scenarios.
- The name argument is more complex than a bare name or a single string literal. Replacing it can
    remove comments between adjacent literals or skip side effects.

## Rule history

- v0.0.108: Split out from `mismatched-type-name`.
