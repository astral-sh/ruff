# Length (`len()`)

## Literal and constructed iterables

### Strings and bytes literals

```py
reveal_type(len("no\rmal"))  # revealed: Literal[6]
reveal_type(len(r"aw stri\ng"))  # revealed: Literal[10]
reveal_type(len(r"conca\t" "ena\tion"))  # revealed: Literal[14]
reveal_type(len(b"ytes lite" rb"al"))  # revealed: Literal[11]
reveal_type(len("𝒰𝕹🄸©🕲𝕕ℇ"))  # revealed: Literal[7]

reveal_type(  # revealed: Literal[7]
    len(
        """foo
bar"""
    )
)
reveal_type(  # revealed: Literal[9]
    len(
        r"""foo\r
bar"""
    )
)
reveal_type(  # revealed: Literal[7]
    len(
        b"""foo
bar"""
    )
)
reveal_type(  # revealed: Literal[9]
    len(
        rb"""foo\r
bar"""
    )
)
```

### Tuples

```py
reveal_type(len(()))  # revealed: Literal[0]
reveal_type(len((1,)))  # revealed: Literal[1]
reveal_type(len((1, 2)))  # revealed: Literal[2]

# TODO: Handle constructor calls
reveal_type(len(tuple()))  # revealed: int

# TODO: Handle star unpacks; Should be: Literal[0]
reveal_type(len((*[],)))  # revealed: Literal[1]

# TODO: Handle star unpacks; Should be: Literal[1]
reveal_type(  # revealed: Literal[2]
    len(
        (
            *[],
            1,
        )
    )
)

# TODO: Handle star unpacks; Should be: Literal[2]
reveal_type(len((*[], 1, 2)))  # revealed: Literal[3]

# TODO: Handle star unpacks; Should be: Literal[0]
reveal_type(len((*[], *{})))  # revealed: Literal[2]
```

### Lists, sets and dictionaries

```py
reveal_type(len([]))  # revealed: int
reveal_type(len([1]))  # revealed: int
reveal_type(len([1, 2]))  # revealed: int
reveal_type(len([*{}, *dict()]))  # revealed: int

reveal_type(len({}))  # revealed: int
reveal_type(len({**{}}))  # revealed: int
reveal_type(len({**{}, **{}}))  # revealed: int

reveal_type(len({1}))  # revealed: int
reveal_type(len({1, 2}))  # revealed: int
reveal_type(len({*[], 2}))  # revealed: int

reveal_type(len(list()))  # revealed: int
reveal_type(len(set()))  # revealed: int
reveal_type(len(dict()))  # revealed: int
reveal_type(len(frozenset()))  # revealed: int
```

## `__len__`

The returned value of `__len__` is implicitly and recursively converted to `int`.

### Literal integers

```py
from typing import Literal

class Zero:
    def __len__(self) -> Literal[0]: ...

class ZeroOrOne:
    def __len__(self) -> Literal[0, 1]: ...

class ZeroOrTrue:
    def __len__(self) -> Literal[0, True]: ...

class OneOrFalse:
    def __len__(self) -> Literal[1] | Literal[False]: ...

class OneOrFoo:
    def __len__(self) -> Literal[1, "foo"]: ...

class ZeroOrStr:
    def __len__(self) -> Literal[0] | str: ...

reveal_type(len(Zero()))  # revealed: Literal[0]
reveal_type(len(ZeroOrOne()))  # revealed: Literal[0, 1]
reveal_type(len(ZeroOrTrue()))  # revealed: Literal[0, 1]
reveal_type(len(OneOrFalse()))  # revealed: Literal[0, 1]

# TODO: Emit a diagnostic
reveal_type(len(OneOrFoo()))  # revealed: int

# TODO: Emit a diagnostic
reveal_type(len(ZeroOrStr()))  # revealed: int
```

### Literal booleans

```py
from typing import Literal

class LiteralTrue:
    def __len__(self) -> Literal[True]: ...

class LiteralFalse:
    def __len__(self) -> Literal[False]: ...

reveal_type(len(LiteralTrue()))  # revealed: Literal[1]
reveal_type(len(LiteralFalse()))  # revealed: Literal[0]
```

### Enums

```py
from enum import Enum, auto
from typing import Literal

class SomeEnum(Enum):
    AUTO = auto()
    INT = 2
    STR = "4"
    TUPLE = (8, "16")
    INT_2 = 3_2

class Auto:
    def __len__(self) -> Literal[SomeEnum.AUTO]: ...

class Int:
    def __len__(self) -> Literal[SomeEnum.INT]: ...

class Str:
    def __len__(self) -> Literal[SomeEnum.STR]: ...

class Tuple:
    def __len__(self) -> Literal[SomeEnum.TUPLE]: ...

class IntUnion:
    def __len__(self) -> Literal[SomeEnum.INT, SomeEnum.INT_2]: ...

reveal_type(len(Auto()))  # revealed: int
reveal_type(len(Int()))  # revealed: Literal[2]
reveal_type(len(Str()))  # revealed: int
reveal_type(len(Tuple()))  # revealed: int
reveal_type(len(IntUnion()))  # revealed: Literal[2, 32]
```

### Negative integers

```py
from typing import Literal

class Negative:
    def __len__(self) -> Literal[-1]: ...

# TODO: Emit a diagnostic
reveal_type(len(Negative()))  # revealed: int
```

### Wrong signature

```py
from typing import Literal

class SecondOptionalArgument:
    def __len__(self, v: int = 0) -> Literal[0]: ...

class SecondRequiredArgument:
    def __len__(self, v: int) -> Literal[1]: ...

# TODO: Emit a diagnostic
reveal_type(len(SecondOptionalArgument()))  # revealed: Literal[0]

# TODO: Emit a diagnostic
reveal_type(len(SecondRequiredArgument()))  # revealed: Literal[1]
```

### No `__len__`

```py
class NoDunderLen: ...

# TODO: Emit a diagnostic
reveal_type(len(NoDunderLen()))  # revealed: int
```
