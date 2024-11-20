# Type aliases

## Basic

```py
type MyInt = int

# TODO: should be typing.TypeAliasType
reveal_type(MyInt)  # revealed: MyInt

x: MyInt = 1

reveal_type(x)  # revealed: Literal[1]

def f() -> None:
    reveal_type(x)  # revealed: int
```
