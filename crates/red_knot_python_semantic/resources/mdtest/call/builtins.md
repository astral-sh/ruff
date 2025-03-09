# Calling builtins

## `bool` with incorrect arguments

```py
class NotBool:
    __bool__ = None

# error: [too-many-positional-arguments] "Too many positional arguments to class `bool`: expected 1, got 2"
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

Other numbers of arguments are invalid

```py
# error: [too-many-positional-arguments] "Too many positional arguments to overload 1 of class `type`: expected 1, got 2"
# error: [missing-argument] "No argument provided for required parameter `dict` of overload 2 of class `type`"
type("Foo", ())

# error: [too-many-positional-arguments] "Too many positional arguments to overload 1 of class `type`: expected 1, got 3"
# error: [unknown-argument] "Argument `weird_other_arg` does not match any known parameter of overload 1 of class `type`"
# error: [unknown-argument] "Argument `weird_other_arg` does not match any known parameter of overload 2 of class `type`"
type("Foo", (), {}, weird_other_arg=42)
```
