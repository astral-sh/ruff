# Narrowing for enums in `match` statements

## StrEnum narrowing

`StrEnum` members compare equal to their underlying string values at runtime. Type narrowing should
recognize this.

```toml
[environment]
python-version = "3.11"
```

```py
from enum import StrEnum
from typing import Literal

class Color(StrEnum):
    RED = "r"
    GREEN = "g"
    BLUE = "b"

def test_literal_matches_strenum(x: Literal["g"]):
    match x:
        case Color.RED:
            reveal_type(x)  # revealed: Never
        case Color.GREEN:
            # This branch IS taken at runtime
            reveal_type(x)  # revealed: Literal["g"]
        case Color.BLUE:
            reveal_type(x)  # revealed: Never
        case _:
            reveal_type(x)  # revealed: Never
```

## Enum with custom `__eq__`

If an enum explicitly overrides `__eq__`, narrowing should be disabled if it's unsafe.

```py
from enum import Enum
from typing import Literal

class CustomEqEnum(Enum):
    A = 1
    B = 2

    def __eq__(self, other):
        return True  # Always returns True

def test_custom_eq(x: Literal[1] | Literal[2]):
    match x:
        case CustomEqEnum.A:
            # We cannot narrow here because __eq__ is overridden
            reveal_type(x)  # revealed: Literal[1, 2]
```

## Tagged union narrowing with StrEnum

```toml
[environment]
python-version = "3.11"
```

```py
from enum import StrEnum
from typing import TypedDict

class Tag(StrEnum):
    FOO = "a"
    BAR = "b"

class Foo(TypedDict):
    tag: Tag
    x: int

class Bar(TypedDict):
    tag: Tag
    y: str

def test_string_literal_match(x: Foo | Bar):
    if x["tag"] == Tag.FOO:
        reveal_type(x)  # revealed: Foo | Bar
```
