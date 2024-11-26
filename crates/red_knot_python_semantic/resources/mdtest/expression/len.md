from typing import Literal

# Length (`len()`)


## Literal and constructed iterables


### Strings and bytes literals

```py
reveal_type(len("no\rmal"))                                 # revealed: Literal[6]
reveal_type(len(r"aw stri\ng"))                             # revealed: Literal[10]
reveal_type(len(r"conca\t" "ena\tion"))                     # revealed: Literal[14]
reveal_type(len(b"ytes lite" br"al"))                       # revealed: Literal[11]
reveal_type(len("𝒰𝕹🄸©🕲𝕕ℇ"))                            # revealed: Literal[7]

reveal_type(                                                # revealed: Literal[7]
    len('''foo
bar''')
)
reveal_type(                                                # revealed: Literal[9]
    len(r"""foo\r
bar""")
)
reveal_type(                                                # revealed: Literal[7]
    len(b"""foo
bar""")
)
reveal_type(                                                # revealed: Literal[9]
    len(br"""foo\r
bar""")
)
```


### Tuples

```py
reveal_type(len(()))                                        # revealed: Literal[0]
reveal_type(len((1,)))                                      # revealed: Literal[1]
reveal_type(len((1, 2)))                                    # revealed: Literal[2]

reveal_type(len((*[],)))                                    # revealed: int
reveal_type(len((*[], 1,)))                                 # revealed: int
reveal_type(len((*[], 1, 2)))                               # revealed: int
reveal_type(len((*[], *{})))                                # revealed: int
```


### Lists, sets and dictionaries

```py
reveal_type(len([]))                                        # revealed: int
reveal_type(len([1]))                                       # revealed: int
reveal_type(len([1, 2]))                                    # revealed: int
reveal_type(len([*{}, *dict()]))                            # revealed: int

reveal_type(len({}))                                        # revealed: int
reveal_type(len({**{}}))                                    # revealed: int
reveal_type(len({**{}, **{}}))                              # revealed: int

reveal_type(len({1}))                                       # revealed: int
reveal_type(len({1, 2}))                                    # revealed: int
reveal_type(len({*[], 2}))                                  # revealed: int

reveal_type(len(list()))                                    # revealed: int
reveal_type(len(set()))                                     # revealed: int
reveal_type(len(dict()))                                    # revealed: int
reveal_type(len(frozenset()))                               # revealed: int
```


## `__len__`

The returned value of `__len__` is implicitly and recursively converted to `int`.


### Literal booleans

```py
from typing import Literal


class LiteralTrue:
    def __len__(self) -> Literal[True]: ...

class LiteralFalse:
    def __len__(self) -> Literal[False]: ...


# Should be: Literal[1], Literal[0]
reveal_type(len(LiteralTrue()))                             # revealed: int
reveal_type(len(LiteralFalse()))                            # revealed: int
```


### Enums

```py
from enum import Enum, auto
from typing import Literal


# TODO: Support enums
class SomeEnum(Enum):
    AUTO = auto()
    KNOWN = 2


class Auto:
    def __len__(self) -> Literal[SomeEnum.AUTO]: ...

class Known:
    def __len__(self) -> Literal[SomeEnum.KNOWN]: ...


# Should be: int, Literal[2]
reveal_type(len(Auto()))                                    # revealed: int
reveal_type(len(Known()))                                   # revealed: int
```


### Deep conversion

```py
from typing import Literal


# TODO: Support `__int__`
class A:
    def __int__(self) -> Literal[42]: ...

# TODO: Support `__index__`
class B:
    def __len__(self) -> A: ...
    def __index__(self) -> Literal[37]: ...

class C:
  def __len__(self) -> B: ...


# Should be: Literal[42], Literal[37]
reveal_type(len(B()))                                       # revealed: int
reveal_type(len(C()))                                       # revealed: int
```


### Unsemantical cases

```py
from typing import Literal


class A:
    def __len__(self) -> Literal[-1]: ...


# Should be: Never
reveal_type(len(A()))                                       # revealed: int
```
