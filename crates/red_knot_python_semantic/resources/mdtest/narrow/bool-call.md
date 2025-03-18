## Narrowing for `bool(..)` checks

```py
def _(flag: bool):

    x = 1 if flag else None

    # valid invocation, positive
    reveal_type(x)  # revealed: Literal[1] | None
    if bool(x is not None):
        reveal_type(x)  # revealed: Literal[1]

    # valid invocation, negative
    reveal_type(x)  # revealed: Literal[1] | None
    if not bool(x is not None):
        reveal_type(x)  # revealed: None

    # no args/narrowing
    reveal_type(x)  # revealed: Literal[1] | None
    if not bool():
        reveal_type(x)  # revealed: Literal[1] | None

    # invalid invocation, too many positional args
    reveal_type(x)  # revealed: Literal[1] | None
    # error: [too-many-positional-arguments] "Too many positional arguments to class `bool`: expected 1, got 2"
    if bool(x is not None, 5):
        reveal_type(x)  # revealed: Literal[1] | None

    # invalid invocation, too many kwargs
    reveal_type(x)  # revealed: Literal[1] | None
    # error: [unknown-argument] "Argument `y` does not match any known parameter of class `bool`"
    if bool(x is not None, y=5):
        reveal_type(x)  # revealed: Literal[1] | None
```
