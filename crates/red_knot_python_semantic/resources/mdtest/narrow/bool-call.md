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
    if bool(x is not None, 5):  # TODO diagnostic
        reveal_type(x)  # revealed: Literal[1] | None

    # invalid invocation, too many kwargs
    reveal_type(x)  # revealed: Literal[1] | None
    if bool(x is not None, y=5):  # TODO diagnostic
        reveal_type(x)  # revealed: Literal[1] | None
```
