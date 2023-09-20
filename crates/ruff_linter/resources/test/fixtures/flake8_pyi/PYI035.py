__all__: list[str]

__all__: list[str] = ["foo"]


class Foo:
    __all__: list[str]
    __match_args__: tuple[str, ...]
    __slots__: tuple[str, ...]


class Bar:
    __all__: list[str] = ["foo"]
    __match_args__: tuple[str, ...] = (1,)
    __slots__: tuple[str, ...] = "foo"
