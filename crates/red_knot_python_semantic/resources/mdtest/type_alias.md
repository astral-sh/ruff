# Type aliases

## Basic

```py
type IntOrStr = int | str

reveal_type(IntOrStr)  # revealed: typing.TypeAliasType
reveal_type(IntOrStr.__name__)  # revealed: Literal["IntOrStr"]
reveal_type(IntOrStr.__value__)  # revealed: int | str

x: IntOrStr = 1

reveal_type(x)  # revealed: Literal[1]

def f() -> None:
    reveal_type(x)  # revealed: int | str
```
