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


# https://github.com/astral-sh/ruff/issues/18950
@dataclass
class Foo:
    def __post_init__(self, bar: int = (x := 1)) -> None:
        pass


@dataclass
class Foo:
    def __post_init__(
        self,
        bar: int = (x := 1)  #  comment
        ,
        baz: int = (y := 2),  # comment
        foo = (a := 1)  #  comment
        ,
        faz = (b := 2),  # comment
    ) -> None:
        pass


@dataclass
class Foo:
    def __post_init__(
        self,
        bar: int = 1,  # comment
        baz: int = 2,  # comment
    ) -> None:
        pass


@dataclass
class Foo:
    def __post_init__(
        self,
        arg1: int = (1)  # comment
        ,
        arg2: int = ((1))  # comment
        ,
        arg2: int = (i for i in range(10))  # comment
        ,
    ) -> None:
        pass


# makes little sense, but is valid syntax
def fun_with_python_syntax():
    @dataclass
    class Foo:
        def __post_init__(
            self,
            bar: (int) = (yield from range(5))  # comment
            ,
        ) -> None:
            ...

    return Foo


@dataclass
class C:
    def __post_init__(self, x: tuple[int, ...] = (
        1,
        2,
    )) -> None:
        self.x = x


@dataclass
class D:
    def __post_init__(self, x: int = """
    """) -> None:
        self.x = x
