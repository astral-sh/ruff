## What it does

Checks for functions and methods with only one `@overload` declaration.

## Why is this bad?

An overload describes one of several ways to call a function. If one signature describes all valid
calls, keep the implementation and remove the overload declaration. In a stub file, keep the
declaration and remove the `@overload` decorator.

## Example

```py
from typing import overload


# This overload definition can be removed.
@overload
def foo(x: int) -> int: ...  # error: [single-overload]
def foo(x: int) -> int:
    return x
```

## References

- [Typing specification: Invalid overload definitions](https://typing.python.org/en/latest/spec/overload.html#invalid-overload-definitions)

## Rule history

- v0.0.100: ty used to have a rule called `invalid-overload` that covered `single-overload`,
    `missing-overload-implementation`, `inconsistent-overload-implementation`,
    `inconsistent-overload-decorator`, and `misplaced-overload-decorator`.

    The old `invalid-overload` rule was removed. If you previously had any configuration for
    `invalid-overload` specifically, you should go through the five rules to see if you want to
    configure them in the same way.
