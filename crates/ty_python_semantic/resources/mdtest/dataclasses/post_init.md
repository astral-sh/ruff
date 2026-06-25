# Tests for the `__post_init__` method

At runtime, if a dataclass has a `__post_init__` method then all `InitVar`-annotated fields will be
passed as positional arguments to that method as the final statement in the dataclass's generated
`__init__` method. The `__post_init__` signature must therefore be compatible with the fields of the
dataclass:

```py
from dataclasses import dataclass, InitVar

@dataclass
class Empty1:
    def __post_init__(self): ...  # fine

@dataclass
class Empty2:
    def __post_init__(self) -> None: ...  # fine

@dataclass
class Empty3:
    # The returned value is discarded,
    # so arbitrary return annotations are allowed
    def __post_init__(self) -> int:
        return 42

@dataclass
class Empty4:
    def __post_init__(self, *args): ...  # fine

@dataclass
class Empty5:
    def __post_init__(self, **kwargs): ...  # fine

@dataclass
class Empty6:
    def __post_init__(self, *args, **kargs): ...  # fine

@dataclass
class Empty7:
    # no arguments will be passed to this method at runtime,
    # because there are no `InitVar` fields on the class,
    # so this is an error:
    #
    # error: [invalid-dataclass]
    def __post_init__(self, required_argument: int): ...

@dataclass
class Empty8:
    # error: [invalid-dataclass]
    def __post_init__(self, *, required_argument): ...

@dataclass
class Empty9:
    # error: [invalid-dataclass]
    def __post_init__(self, required_argument, /): ...

@dataclass
class SingleField:
    x: int

    # `x` will not be passed to `__post_init__`,
    # because it is not an `InitVar`, so this is an
    #
    # error: [invalid-dataclass]
    def __post_init__(self, x: int) -> None: ...

@dataclass
class SingleFieldGood:
    x: int

    # this is fine!
    def __post_init__(self) -> None: ...

@dataclass
class HasInitVarNoParameter:
    x: InitVar[int]

    # error: [invalid-dataclass]
    def __post_init__(self) -> None: ...

@dataclass
class HasInitVarDifferentParameterName:
    x: InitVar[int]

    # because arguments are always passed in positionally
    # to `__post_init__` methods, we allow a parameter to
    # have an arbitrary name as long as it is inferred has
    # having a compatible type. So this is fine:
    def __post_init__(self, xx) -> None: ...

@dataclass
class HasInitVarBadParameterType:
    x: InitVar[int]

    # error: [invalid-dataclass]
    def __post_init__(self, x: str) -> None: ...

@dataclass
class HasInitVarBadParameterKind:
    x: InitVar[int]

    # error: [invalid-dataclass]
    def __post_init__(self, *, x: int) -> None: ...

@dataclass
class HasInitVarGood:
    x: InitVar[int]

    def __post_init__(self, x: int) -> None: ...

@dataclass
class HasInitVarGoodPositionalOnly:
    x: InitVar[int]

    # arguments are always passed to `__post_init__` positionally at runtime,
    # so this is fine
    def __post_init__(self, x: int, /) -> None: ...

@dataclass
class LotsOfInitVarsBad:
    a: int
    b: InitVar[str]
    c: InitVar[bytes]
    d: int
    e: int
    f: InitVar[range]
    g: int

    # Only `InitVar` fields are passed in at runtime, so this is an
    # error: [invalid-dataclass]
    def __post_init__(self, a: int, b: str, c: bytes, d: int, e: int, f: range, g: int): ...

@dataclass
class LotsOfInitVarsOutOfOrder:
    a: int
    b: InitVar[str]
    c: InitVar[bytes]
    d: int
    e: int
    f: InitVar[range]
    g: int

    # the parameters are in the wrong order, so this is an
    # error: [invalid-dataclass]
    def __post_init__(self, c: bytes, b: str, f: range): ...

@dataclass
class LotsOfInitVarsGood:
    a: int
    b: InitVar[str]
    c: InitVar[bytes]
    d: int
    e: int
    f: InitVar[range]
    g: int

    def __post_init__(self, b: str, c: bytes, f: range): ...

@dataclass
class InitVarSubclassGood(LotsOfInitVarsGood):
    h: InitVar[list[int]]
    i: str
    j: InitVar[bool]

    def __post_init__(self, b: str, c: bytes, f: range, h: list[int], j: bool): ...

@dataclass
class InitVarSubclassBad(LotsOfInitVarsGood):
    h: InitVar[list[int]]
    i: str
    j: InitVar[bool]

    # error: [invalid-dataclass]
    def __post_init__(self, h: list[int], j: bool, b: str, c: bytes, f: range): ...
```
