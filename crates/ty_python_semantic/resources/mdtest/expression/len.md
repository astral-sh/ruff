# Length (`len()`)

## Literal and constructed iterables

### Strings and bytes literals

```py
reveal_type(len("no\rmal"))  # revealed: Literal[6]
reveal_type(len(r"aw stri\ng"))  # revealed: Literal[10]
reveal_type(len(r"conca\t" "ena\tion"))  # revealed: Literal[14]
reveal_type(len(b"ytes lite" rb"al"))  # revealed: Literal[11]
reveal_type(len("𝒰𝕹🄸©🕲𝕕ℇ"))  # revealed: Literal[7]

# fmt: off

reveal_type(len(  # revealed: Literal[7]
        """foo
bar"""
))

reveal_type(len(  # revealed: Literal[9]
        r"""foo\r
bar"""
))

reveal_type(len(  # revealed: Literal[7]
        b"""foo
bar"""
))
reveal_type(len(  # revealed: Literal[9]
        rb"""foo\r
bar"""
))

# fmt: on
```

### Tuples

```py
reveal_type(len(()))  # revealed: Literal[0]
reveal_type(len((1,)))  # revealed: Literal[1]
reveal_type(len((1, 2)))  # revealed: Literal[2]
reveal_type(len(tuple()))  # revealed: Literal[0]

# TODO: Handle star unpacks; Should be: Literal[0]
reveal_type(len((*[],)))  # revealed: Literal[1]

# fmt: off

# TODO: Handle star unpacks; Should be: Literal[1]
reveal_type(len(  # revealed: Literal[2]
    (
        *[],
        1,
    )
))

# fmt: on

# TODO: Handle star unpacks; Should be: Literal[2]
reveal_type(len((*[], 1, 2)))  # revealed: Literal[3]

# TODO: Handle star unpacks; Should be: Literal[0]
reveal_type(len((*[], *{})))  # revealed: Literal[2]
```

Tuple subclasses:

```py
class EmptyTupleSubclass(tuple[()]): ...
class Length1TupleSubclass(tuple[int]): ...
class Length2TupleSubclass(tuple[int, str]): ...
class UnknownLengthTupleSubclass(tuple[int, ...]): ...

reveal_type(len(EmptyTupleSubclass()))  # revealed: Literal[0]
reveal_type(len(Length1TupleSubclass((1,))))  # revealed: Literal[1]
reveal_type(len(Length2TupleSubclass((1, "foo"))))  # revealed: Literal[2]
reveal_type(len(UnknownLengthTupleSubclass((1, 2, 3))))  # revealed: int

reveal_type(tuple[int, int].__len__)  # revealed: (self: tuple[int, int], /) -> Literal[2]
reveal_type(tuple[int, ...].__len__)  # revealed: (self: tuple[int, ...], /) -> int

def f(x: tuple[int, int], y: tuple[int, ...]):
    reveal_type(x.__len__)  # revealed: () -> Literal[2]
    reveal_type(y.__len__)  # revealed: () -> int

reveal_type(EmptyTupleSubclass.__len__)  # revealed: (self: tuple[()], /) -> Literal[0]
reveal_type(EmptyTupleSubclass().__len__)  # revealed: () -> Literal[0]
reveal_type(UnknownLengthTupleSubclass.__len__)  # revealed: (self: tuple[int, ...], /) -> int
reveal_type(UnknownLengthTupleSubclass().__len__)  # revealed: () -> int
```

If `__len__` is overridden, we use the overridden return type:

```py
from typing import Literal

class UnknownLengthSubclassWithDunderLenOverridden(tuple[int, ...]):
    def __len__(self) -> Literal[42]:
        return 42

reveal_type(len(UnknownLengthSubclassWithDunderLenOverridden()))  # revealed: Literal[42]

class FixedLengthSubclassWithDunderLenOverridden(tuple[int]):
    def __len__(self) -> Literal[42]:  # error: [invalid-method-override]
        return 42

reveal_type(len(FixedLengthSubclassWithDunderLenOverridden((1,))))  # revealed: Literal[42]
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
    def __len__(self) -> Literal[0]:
        return 0

class ZeroOrOne:
    def __len__(self) -> Literal[0, 1]:
        return 0

class ZeroOrTrue:
    def __len__(self) -> Literal[0, True]:
        return 0

class OneOrFalse:
    def __len__(self) -> Literal[1] | Literal[False]:
        return 1

class OneOrFoo:
    def __len__(self) -> Literal[1, "foo"]:
        return 1

class ZeroOrStr:
    def __len__(self) -> Literal[0] | str:
        return 0

reveal_type(len(Zero()))  # revealed: Literal[0]
reveal_type(len(ZeroOrOne()))  # revealed: Literal[0, 1]
reveal_type(len(ZeroOrTrue()))  # revealed: Literal[0, 1]
reveal_type(len(OneOrFalse()))  # revealed: Literal[1, 0]

# error: [invalid-argument-type] "Argument to function `len` is incorrect: Expected `Sized`, found `OneOrFoo`"
reveal_type(len(OneOrFoo()))  # revealed: int

# error: [invalid-argument-type] "Argument to function `len` is incorrect: Expected `Sized`, found `ZeroOrStr`"
reveal_type(len(ZeroOrStr()))  # revealed: int
```

### Literal booleans

```py
from typing import Literal

class LiteralTrue:
    def __len__(self) -> Literal[True]:
        return True

class LiteralFalse:
    def __len__(self) -> Literal[False]:
        return False

reveal_type(len(LiteralTrue()))  # revealed: Literal[1]
reveal_type(len(LiteralFalse()))  # revealed: Literal[0]
```

### Negative integers

```py
from typing import Literal

class Negative:
    def __len__(self) -> Literal[-1]:
        return -1

# TODO: Emit a diagnostic
reveal_type(len(Negative()))  # revealed: int
```

### Wrong signature

```py
from typing import Literal

class SecondOptionalArgument:
    def __len__(self, v: int = 0) -> Literal[0]:
        return 0

class SecondRequiredArgument:
    def __len__(self, v: int) -> Literal[1]:
        return 1

# this is fine: the call succeeds at runtime since the second argument is optional
reveal_type(len(SecondOptionalArgument()))  # revealed: Literal[0]

# error: [invalid-argument-type] "Argument to function `len` is incorrect: Expected `Sized`, found `SecondRequiredArgument`"
reveal_type(len(SecondRequiredArgument()))  # revealed: int
```

### No `__len__`

```py
class NoDunderLen: ...

# error: [invalid-argument-type]
reveal_type(len(NoDunderLen()))  # revealed: int
```
