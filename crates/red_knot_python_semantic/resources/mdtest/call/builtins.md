# Calling builtins

## `bool` with incorrect arguments

```py
class NotBool:
    __bool__ = None

# TODO: We should emit an `invalid-argument` error here for `2` because `bool` only takes one argument.
bool(1, 2)

# TODO: We should emit an `unsupported-bool-conversion` error here because the argument doesn't implement `__bool__` correctly.
bool(NotBool())
```

## Calls to `type()`

A single-argument call to `type()` returns an object that has the argument's meta-type. (This is
tested more extensively in `crates/red_knot_python_semantic/resources/mdtest/attributes.md`,
alongside the tests for the `__class__` attribute.)

```py
reveal_type(type(1))  # revealed: Literal[int]
```

But a three-argument call to type creates a dynamic instance of the `type` class:

```py
reveal_type(type("Foo", (), {}))  # revealed: type
```

Other numbers of arguments are invalid (TODO -- these should emit a diagnostic)

```py
type("Foo", ())
type("Foo", (), {}, weird_other_arg=42)
```
