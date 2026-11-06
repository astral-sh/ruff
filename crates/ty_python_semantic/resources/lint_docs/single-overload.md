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

## Migrating from `invalid-overload`

In 0.0.100, `invalid-overload` was split into `single-overload`, `missing-overload-implementation`,
`inconsistent-overload-implementation`, `inconsistent-overload-decorator`, and
`misplaced-overload-decorator`.

## References

- [Typing specification: Invalid overload definitions](https://typing.python.org/en/latest/spec/overload.html#invalid-overload-definitions)
