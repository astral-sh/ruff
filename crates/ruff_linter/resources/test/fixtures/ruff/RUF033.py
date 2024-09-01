from dataclasses import InitVar, dataclass


# OK
@dataclass
class Foo:
    bar: InitVar[int] = 0
    baz: InitVar[int] = 1

    def __post_init__(self, bar, baz) -> None: ...


# RUF033
@dataclass
class Foo:
    bar: InitVar[int] = 0
    baz: InitVar[int] = 1

    def __post_init__(self, bar = 11, baz = 11) -> None: ...


# RUF033
@dataclass
class Foo:
    def __post_init__(self, bar = 11, baz = 11) -> None: ...


# OK
@dataclass
class Foo:
    def __something_else__(self, bar = 11, baz = 11) -> None: ...


# OK
def __post_init__(foo: bool = True) -> None: ...


# OK
class Foo:
    def __post_init__(self, x="hmm") -> None: ...


# RUF033
@dataclass
class Foo:
    def __post_init__(self, bar: int = 11, baz: Something[Whatever | None] = 11) -> None: ...


# RUF033
@dataclass
class Foo:
    """A very helpful docstring.

    Docstrings are very important and totally not a waste of time.
    """

    ping = "pong"

    def __post_init__(self, bar: int = 11, baz: int = 12) -> None: ...


# RUF033
@dataclass
class Foo:
    bar = "should've used attrs"

    def __post_init__(self, bar: str = "ahhh", baz: str = "hmm") -> None: ...
